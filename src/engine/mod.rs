//! Supervises a KataGo `analysis` subprocess and multiplexes queries over its
//! stdin/stdout JSON protocol.
//!
//! Every query carries a unique `id`; every response line echoes it. Responses
//! are routed to the waiting caller through a per-id channel, which lets a
//! single query produce several results (one per analysed turn). When KataGo
//! exits, all waiters fail immediately and a supervisor task restarts it with
//! exponential backoff.

pub mod protocol;

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Notify, mpsc};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::api::types::AnalysisResponse;
use crate::config::KatagoConfig;
use crate::error::{EngineError, Result};

pub use protocol::Query;

/// How long to wait for KataGo's first answer, which includes loading the network.
const READINESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Timeout for administrative actions such as `clear_cache`.
const ACTION_TIMEOUT: Duration = Duration::from_secs(30);
/// How long to wait for KataGo to exit on its own during shutdown before killing it.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// Longest pause between restart attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Number of stderr lines remembered for post-mortem logging.
const STDERR_TAIL_LINES: usize = 40;

/// KataGo build information as reported by `query_version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KatagoVersion {
    /// Version string, e.g. `"1.18.2"`.
    pub version: String,
    /// Git hash, when the build recorded one.
    pub git_hash: Option<String>,
}

/// Snapshot of the engine's state.
#[derive(Debug, Clone)]
pub struct EngineStatus {
    /// KataGo process is running.
    pub alive: bool,
    /// KataGo has answered a query since it was last started.
    pub ready: bool,
    /// Restarts performed since the server started.
    pub restarts: u32,
    /// Configured restart budget.
    pub max_restart_attempts: u32,
    /// Time since the engine was created.
    pub uptime: Duration,
    /// KataGo version once known.
    pub version: Option<KatagoVersion>,
}

/// Handle to the supervised KataGo process. Cheap to clone.
#[derive(Debug, Clone)]
pub struct AnalysisEngine {
    inner: Arc<Inner>,
}

struct Inner {
    config: KatagoConfig,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    child: tokio::sync::Mutex<Option<Child>>,
    pending: Mutex<HashMap<String, mpsc::UnboundedSender<Value>>>,
    alive: AtomicBool,
    ready: AtomicBool,
    shutting_down: AtomicBool,
    restarts: AtomicU32,
    started_at: Instant,
    version: RwLock<Option<KatagoVersion>>,
    exited: Notify,
    stderr_tail: Mutex<VecDeque<String>>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("alive", &self.alive.load(Ordering::SeqCst))
            .field("ready", &self.ready.load(Ordering::SeqCst))
            .field("restarts", &self.restarts.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

/// Removes a pending entry when the waiting request finishes or is cancelled.
///
/// If the request future is dropped before KataGo answered (HTTP timeout,
/// client disconnect), KataGo is told to stop working on it.
struct PendingGuard {
    inner: Arc<Inner>,
    id: String,
    finished: bool,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.inner.pending.lock() {
            map.remove(&self.id);
        }
        if !self.finished && !self.inner.shutting_down.load(Ordering::SeqCst) {
            let inner = Arc::clone(&self.inner);
            let id = std::mem::take(&mut self.id);
            tokio::spawn(async move {
                debug!("request {id} abandoned before completion, terminating it in KataGo");
                inner.terminate(&id).await;
            });
        }
    }
}

impl AnalysisEngine {
    /// Spawns KataGo and starts the supervisor. Fails only if the very first
    /// spawn fails (for example because the binary does not exist).
    pub async fn start(config: KatagoConfig) -> Result<Self> {
        let inner = Arc::new(Inner {
            config,
            stdin: tokio::sync::Mutex::new(None),
            child: tokio::sync::Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            restarts: AtomicU32::new(0),
            started_at: Instant::now(),
            version: RwLock::new(None),
            exited: Notify::new(),
            stderr_tail: Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)),
        });
        Inner::spawn_child(&inner).await?;
        tokio::spawn(Inner::supervise(Arc::clone(&inner)));
        Ok(Self { inner })
    }

    /// Runs an analysis query and returns one result per analysed turn,
    /// ordered by turn number.
    pub async fn analyze(&self, query: &Query) -> Result<Vec<AnalysisResponse>> {
        let expected = query.expected_results();
        let value = serde_json::to_value(query)?;
        let per_result_timeout = Duration::from_secs(self.inner.config.move_timeout_secs);
        let started = Instant::now();

        let outcome = self
            .inner
            .request(&query.id, &value, expected, per_result_timeout)
            .await;

        let label = match &outcome {
            Ok(_) => "ok",
            Err(EngineError::Timeout(_)) => "timeout",
            Err(EngineError::Rejected { .. }) => "rejected",
            Err(EngineError::ProcessDied | EngineError::ShuttingDown) => "unavailable",
            Err(_) => "error",
        };
        metrics::counter!("katago_analysis_requests_total", "outcome" => label).increment(1);
        metrics::histogram!("katago_analysis_duration_seconds", "outcome" => label)
            .record(started.elapsed().as_secs_f64());

        let mut results = outcome?
            .into_iter()
            .map(|v| {
                serde_json::from_value::<AnalysisResponse>(v)
                    .map_err(|e| EngineError::Parse(e.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        results.sort_by_key(|r| r.turn_number);
        Ok(results)
    }

    /// Asks KataGo for its version. Cached after the first successful answer.
    pub async fn query_version(&self) -> Result<KatagoVersion> {
        if let Some(v) = self.inner.cached_version() {
            return Ok(v);
        }
        self.inner.fetch_version(ACTION_TIMEOUT).await
    }

    /// Clears KataGo's neural network cache and waits for the acknowledgement.
    pub async fn clear_cache(&self) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let query = json!({ "id": id, "action": "clear_cache" });
        self.inner.request(&id, &query, 1, ACTION_TIMEOUT).await?;
        info!("KataGo cache cleared");
        Ok(())
    }

    /// Current state, for health reporting.
    pub fn status(&self) -> EngineStatus {
        EngineStatus {
            alive: self.inner.alive.load(Ordering::SeqCst),
            ready: self.inner.ready.load(Ordering::SeqCst),
            restarts: self.inner.restarts.load(Ordering::SeqCst),
            max_restart_attempts: self.inner.config.max_restart_attempts,
            uptime: self.inner.started_at.elapsed(),
            version: self.inner.cached_version(),
        }
    }

    /// The KataGo configuration in use.
    pub fn config(&self) -> &KatagoConfig {
        &self.inner.config
    }

    /// Stops accepting work, asks KataGo to terminate, and reaps the process.
    pub async fn shutdown(&self) {
        let inner = &self.inner;
        inner.shutting_down.store(true, Ordering::SeqCst);
        if inner.alive.load(Ordering::SeqCst) {
            let terminate =
                json!({ "id": uuid::Uuid::new_v4().to_string(), "action": "terminate_all" });
            if let Err(e) = inner.write_line(&terminate).await {
                debug!("could not send terminate_all during shutdown: {e}");
            }
        }
        // Closing stdin makes KataGo exit once it has flushed its output.
        *inner.stdin.lock().await = None;
        inner.pending.lock().map(|mut p| p.clear()).ok();
        if let Some(status) = inner.reap(SHUTDOWN_GRACE).await {
            info!("KataGo exited during shutdown with {status}");
        } else {
            debug!("no KataGo process to reap during shutdown");
        }
        inner.alive.store(false, Ordering::SeqCst);
        inner.ready.store(false, Ordering::SeqCst);
        metrics::gauge!("katago_engine_up").set(0.0);
    }
}

impl Inner {
    fn cached_version(&self) -> Option<KatagoVersion> {
        self.version.read().ok().and_then(|v| v.clone())
    }

    async fn fetch_version(self: &Arc<Self>, wait: Duration) -> Result<KatagoVersion> {
        let id = uuid::Uuid::new_v4().to_string();
        let query = json!({ "id": id, "action": "query_version" });
        let mut responses = self.request(&id, &query, 1, wait).await?;
        let response = responses
            .pop()
            .ok_or_else(|| EngineError::Parse("empty query_version response".into()))?;
        let version = response
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::Parse("query_version response lacks version".into()))?
            .to_owned();
        let git_hash = response
            .get("git_hash")
            .and_then(Value::as_str)
            .filter(|h| !h.is_empty() && *h != "<omitted>")
            .map(ToOwned::to_owned);
        let parsed = KatagoVersion { version, git_hash };
        if let Ok(mut slot) = self.version.write() {
            *slot = Some(parsed.clone());
        }
        Ok(parsed)
    }

    async fn spawn_child(self: &Arc<Self>) -> Result<()> {
        let cfg = &self.config;
        info!(
            katago = %cfg.katago_path.display(),
            model = %cfg.model_path.display(),
            human_model = ?cfg.human_model_path.as_ref().map(|p| p.display().to_string()),
            config = %cfg.config_path.display(),
            "starting KataGo analysis engine"
        );

        let mut command = Command::new(&cfg.katago_path);
        command.arg("analysis").arg("-model").arg(&cfg.model_path);
        if let Some(human) = &cfg.human_model_path {
            command.arg("-human-model").arg(human);
        }
        command
            .arg("-config")
            .arg(&cfg.config_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command.spawn().map_err(|e| {
            EngineError::ProcessStartFailed(format!("{}: {e}", cfg.katago_path.display()))
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EngineError::ProcessStartFailed("no stdout pipe".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| EngineError::ProcessStartFailed("no stderr pipe".into()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::ProcessStartFailed("no stdin pipe".into()))?;

        *self.stdin.lock().await = Some(stdin);
        *self.child.lock().await = Some(child);
        self.stderr_tail.lock().map(|mut t| t.clear()).ok();
        self.ready.store(false, Ordering::SeqCst);
        self.alive.store(true, Ordering::SeqCst);
        metrics::gauge!("katago_engine_up").set(1.0);

        tokio::spawn(Self::read_stderr(Arc::clone(self), stderr));
        tokio::spawn(Self::read_stdout(Arc::clone(self), stdout));
        tokio::spawn(Self::probe_ready(Arc::clone(self)));
        Ok(())
    }

    async fn probe_ready(self: Arc<Self>) {
        match self.fetch_version(READINESS_TIMEOUT).await {
            Ok(v) => {
                self.ready.store(true, Ordering::SeqCst);
                info!(version = %v.version, git_hash = ?v.git_hash, "KataGo is ready");
            }
            Err(EngineError::ProcessDied | EngineError::ShuttingDown) => {}
            Err(e) => warn!("KataGo readiness probe failed: {e}"),
        }
    }

    async fn read_stderr(self: Arc<Self>, stderr: tokio::process::ChildStderr) {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            debug!(target: "katago::stderr", "{line}");
            if let Ok(mut tail) = self.stderr_tail.lock() {
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        }
    }

    async fn read_stdout(self: Arc<Self>, stdout: ChildStdout) {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => self.route_line(&line),
                Ok(None) => break,
                Err(e) => {
                    error!("error reading KataGo stdout: {e}");
                    break;
                }
            }
        }
        self.on_exit();
    }

    fn route_line(&self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            debug!(target: "katago::stdout", "non-JSON output: {trimmed}");
            return;
        };
        match value.get("id").and_then(Value::as_str) {
            Some(id) => {
                let sender = self.pending.lock().ok().and_then(|p| p.get(id).cloned());
                if let Some(tx) = sender {
                    let _ = tx.send(value);
                } else {
                    debug!("response for unknown or finished query id {id}");
                }
            }
            None => {
                if let Some(err) = value.get("error") {
                    warn!("KataGo reported an error without a query id: {err}");
                } else {
                    debug!(target: "katago::stdout", "message without id: {trimmed}");
                }
            }
        }
    }

    fn on_exit(&self) {
        let was_alive = self.alive.swap(false, Ordering::SeqCst);
        self.ready.store(false, Ordering::SeqCst);
        metrics::gauge!("katago_engine_up").set(0.0);
        // Dropping the senders wakes every waiter with `None` -> ProcessDied.
        if let Ok(mut pending) = self.pending.lock() {
            let dropped = pending.len();
            pending.clear();
            if dropped > 0 {
                warn!("failed {dropped} in-flight request(s) because KataGo exited");
            }
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            info!("KataGo exited");
        } else if was_alive {
            error!("KataGo exited unexpectedly");
            if let Ok(tail) = self.stderr_tail.lock() {
                for line in tail.iter() {
                    error!(target: "katago::stderr", "{line}");
                }
            }
        }
        self.exited.notify_one();
    }

    async fn reap(&self, grace: Duration) -> Option<std::process::ExitStatus> {
        let child = self.child.lock().await.take();
        let mut child = child?;
        match timeout(grace, child.wait()).await {
            Ok(Ok(status)) => Some(status),
            Ok(Err(e)) => {
                warn!("failed to wait for KataGo: {e}");
                None
            }
            Err(_) => {
                warn!("KataGo did not exit within {grace:?}, killing it");
                if let Err(e) = child.kill().await {
                    warn!("failed to kill KataGo: {e}");
                }
                child.wait().await.ok()
            }
        }
    }

    async fn supervise(self: Arc<Self>) {
        loop {
            self.exited.notified().await;
            if self.shutting_down.load(Ordering::SeqCst) {
                return;
            }
            if let Some(status) = self.reap(SHUTDOWN_GRACE).await {
                warn!("KataGo exit status: {status}");
            }

            let mut consecutive_failures: u32 = 0;
            loop {
                if self.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                let restarts = self.restarts.load(Ordering::SeqCst);
                if restarts >= self.config.max_restart_attempts {
                    error!(
                        "KataGo restarted {restarts} time(s), which is the configured limit; \
                         leaving it stopped (health reports unhealthy)"
                    );
                    return;
                }
                let backoff =
                    Duration::from_secs(2u64.saturating_pow(consecutive_failures)).min(MAX_BACKOFF);
                warn!(
                    "restarting KataGo in {backoff:?} (restart {}/{})",
                    restarts + 1,
                    self.config.max_restart_attempts
                );
                tokio::time::sleep(backoff).await;
                if self.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                self.restarts.fetch_add(1, Ordering::SeqCst);
                metrics::counter!("katago_engine_restarts_total").increment(1);
                match Self::spawn_child(&self).await {
                    Ok(()) => {
                        info!("KataGo restarted");
                        break;
                    }
                    Err(e) => {
                        error!("KataGo restart failed: {e}");
                        consecutive_failures = consecutive_failures.saturating_add(1);
                    }
                }
            }
        }
    }

    /// Writes one JSON line to KataGo's stdin without checking engine state.
    async fn write_line(&self, value: &Value) -> Result<()> {
        let mut line = serde_json::to_vec(value)?;
        line.push(b'\n');
        let mut guard = self.stdin.lock().await;
        let stdin = guard.as_mut().ok_or(EngineError::ProcessDied)?;
        stdin.write_all(&line).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn send(&self, value: &Value) -> Result<()> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(EngineError::ShuttingDown);
        }
        if !self.alive.load(Ordering::SeqCst) {
            return Err(EngineError::ProcessDied);
        }
        debug!(target: "katago::stdin", "{value}");
        self.write_line(value).await.map_err(|e| match e {
            EngineError::Io(io) => {
                warn!("writing to KataGo failed: {io}");
                EngineError::ProcessDied
            }
            other => other,
        })
    }

    /// Sends a query and collects `expected` final results for it.
    ///
    /// `per_result_timeout` is the maximum silence tolerated between results;
    /// on expiry the query is terminated inside KataGo. If the returned future
    /// is dropped early the query is terminated as well.
    async fn request(
        self: &Arc<Self>,
        id: &str,
        query: &Value,
        expected: usize,
        per_result_timeout: Duration,
    ) -> Result<Vec<Value>> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.pending
            .lock()
            .map_err(|_| EngineError::Parse("pending map poisoned".into()))?
            .insert(id.to_owned(), tx);
        let mut guard = PendingGuard {
            inner: Arc::clone(self),
            id: id.to_owned(),
            finished: false,
        };

        if let Err(e) = self.send(query).await {
            guard.finished = true;
            return Err(e);
        }
        let outcome = self
            .collect(id, &mut rx, expected, per_result_timeout)
            .await;
        guard.finished = true;
        outcome
    }

    async fn collect(
        &self,
        id: &str,
        rx: &mut mpsc::UnboundedReceiver<Value>,
        expected: usize,
        per_result_timeout: Duration,
    ) -> Result<Vec<Value>> {
        let mut results = Vec::with_capacity(expected);
        loop {
            let message = match timeout(per_result_timeout, rx.recv()).await {
                Ok(Some(message)) => message,
                Ok(None) => return Err(EngineError::ProcessDied),
                Err(_) => {
                    self.terminate(id).await;
                    return Err(EngineError::Timeout(per_result_timeout.as_secs()));
                }
            };

            if let Some(warning) = message.get("warning") {
                warn!(
                    field = ?message.get("field").and_then(serde_json::Value::as_str),
                    "KataGo warning for query {id}: {warning}"
                );
                continue;
            }
            if let Some(err) = message.get("error") {
                let text = err
                    .as_str()
                    .map_or_else(|| err.to_string(), ToOwned::to_owned);
                let field = message
                    .get("field")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                return Err(match field {
                    Some(_) => EngineError::Rejected {
                        message: text,
                        field,
                    },
                    None => EngineError::Katago(text),
                });
            }
            if message.get("action").is_some() {
                results.push(message);
                return Ok(results);
            }
            if message.get("isDuringSearch").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            results.push(message);
            if results.len() >= expected {
                return Ok(results);
            }
        }
    }

    async fn terminate(&self, id: &str) {
        let query = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "action": "terminate",
            "terminateId": id,
        });
        if let Err(e) = self.send(&query).await {
            debug!("could not terminate query {id}: {e}");
        }
    }
}
