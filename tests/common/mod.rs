//! Shared harness: boots the router in-process against the fake KataGo.

#![allow(dead_code, unreachable_pub)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt as _;
use metrics_exporter_prometheus::PrometheusHandle;
use tempfile::TempDir;
use tower::ServiceExt as _;

use katago_server::api::{AppState, build_router};
use katago_server::config::Config;
use katago_server::engine::AnalysisEngine;

static METRICS: OnceLock<PrometheusHandle> = OnceLock::new();

fn metrics_handle() -> PrometheusHandle {
    METRICS
        .get_or_init(|| {
            katago_server::metrics::install_global()
                .unwrap_or_else(|_| katago_server::metrics::detached_handle())
        })
        .clone()
}

/// A running server plus the files it depends on.
pub struct TestServer {
    pub app: Router,
    pub engine: AnalysisEngine,
    pub config: Arc<Config>,
    pub query_log: PathBuf,
    _dir: TempDir,
}

/// Options for the fake KataGo wrapper.
#[derive(Default)]
pub struct FakeOptions<'a> {
    pub env: &'a [(&'a str, &'a str)],
}

pub fn fake_katago_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake_katago.py")
}

/// Writes a wrapper script that sets environment for the fake and execs it.
pub fn write_wrapper(dir: &Path, log: &Path, env: &[(&str, &str)]) -> PathBuf {
    use std::fmt::Write as _;
    let wrapper = dir.join("katago");
    let mut script = String::from("#!/bin/sh\n");
    let _ = writeln!(script, "export FAKE_KATAGO_LOG='{}'", log.display());
    for (k, v) in env {
        let _ = writeln!(script, "export {k}='{v}'");
    }
    let _ = writeln!(
        script,
        "exec python3 '{}' \"$@\"",
        fake_katago_script().display()
    );
    std::fs::write(&wrapper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    wrapper
}

/// Prepares a config pointing at a fake KataGo in a fresh temp dir.
pub fn fake_config(dir: &TempDir, options: &FakeOptions<'_>) -> (Config, PathBuf) {
    let log = dir.path().join("queries.log");
    let wrapper = write_wrapper(dir.path(), &log, options.env);
    let model = dir.path().join("model.bin.gz");
    let cfg_file = dir.path().join("analysis.cfg");
    std::fs::write(&model, b"not a real model").unwrap();
    std::fs::write(&cfg_file, b"maxVisits = 10\n").unwrap();

    let mut config = Config::default();
    config.katago.katago_path = wrapper;
    config.katago.model_path = model;
    config.katago.config_path = cfg_file;
    config.katago.move_timeout_secs = 5;
    config.server.request_timeout_secs = 30;
    (config, log)
}

impl TestServer {
    /// Starts a server with default fake options.
    pub async fn start() -> Self {
        Self::start_with(FakeOptions::default(), |_| {}).await
    }

    /// Starts a server with custom fake options and config tweaks.
    pub async fn start_with(options: FakeOptions<'_>, tweak: impl FnOnce(&mut Config)) -> Self {
        let dir = TempDir::new().unwrap();
        let (mut config, query_log) = fake_config(&dir, &options);
        tweak(&mut config);
        config.validate().unwrap();
        config.validate_paths().unwrap();

        let engine = AnalysisEngine::start(config.katago.clone())
            .await
            .expect("fake katago starts");
        let config = Arc::new(config);
        let app = build_router(AppState {
            engine: engine.clone(),
            config: Arc::clone(&config),
            metrics: metrics_handle(),
        });
        Self {
            app,
            engine,
            config,
            query_log,
            _dir: dir,
        }
    }

    /// Waits until the engine reports ready (or panics after 20s).
    pub async fn wait_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.engine.status().ready {
            assert!(
                Instant::now() < deadline,
                "engine never became ready: {:?}",
                self.engine.status()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Waits until the engine is no longer alive.
    pub async fn wait_dead(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.engine.status().alive {
            assert!(
                Instant::now() < deadline,
                "engine never died: {:?}",
                self.engine.status()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub async fn get(&self, path: &str) -> Response<Body> {
        let request = Request::builder().uri(path).body(Body::empty()).unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    pub async fn post_json(&self, path: &str, body: &serde_json::Value) -> Response<Body> {
        self.post_raw(path, "application/json", body.to_string())
            .await
    }

    pub async fn post_raw(&self, path: &str, content_type: &str, body: String) -> Response<Body> {
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body))
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Every query line the fake received so far, parsed.
    pub fn queries(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.query_log)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}

/// Reads a response body as JSON.
pub async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "body is not JSON ({e}): {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

/// Reads a response body as text.
pub async fn text(response: Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Asserts a problem+json response with the given status and type slug.
pub async fn assert_problem(
    response: Response<Body>,
    status: StatusCode,
    slug: &str,
) -> serde_json::Value {
    assert_eq!(response.status(), status);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    let body = json(response).await;
    let ty = body["type"].as_str().unwrap();
    assert!(
        ty.ends_with(&format!("#{slug}")),
        "type {ty} does not end with #{slug}: {body}"
    );
    assert_eq!(body["status"], status.as_u16());
    assert!(body["title"].is_string());
    assert!(body["detail"].is_string());
    body
}
