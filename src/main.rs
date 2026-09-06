//! `katago-server` binary: `serve` (default), `healthcheck` and `check-config`.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use katago_server::api::{AppState, build_router};
use katago_server::config::{Config, LogFormat};
use katago_server::engine::AnalysisEngine;

static LONG_VERSION: LazyLock<String> = LazyLock::new(|| match katago_server::GIT_SHA {
    Some(sha) => format!("{} ({sha})", katago_server::VERSION),
    None => katago_server::VERSION.to_owned(),
});

#[derive(Debug, Parser)]
#[command(name = "katago-server", about, version = &**LONG_VERSION)]
struct Cli {
    /// Path to config.toml. Defaults to ./config.toml when it exists.
    #[arg(
        long,
        short,
        global = true,
        env = "KATAGO_CONFIG_FILE",
        value_name = "FILE"
    )]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the HTTP server (the default).
    Serve,
    /// Probe a running server and exit 0 if it is healthy. Used by Docker HEALTHCHECK.
    Healthcheck {
        /// Path to request.
        #[arg(long, default_value = "/api/v1/health")]
        path: String,
        /// Seconds to wait for a response.
        #[arg(long, default_value_t = 5)]
        timeout_secs: u64,
    },
    /// Load, validate and print the effective configuration, then exit.
    CheckConfig,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load(cli.config.as_deref())?;
    config.validate()?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(config),
        Command::Healthcheck { path, timeout_secs } => {
            healthcheck(&config, &path, Duration::from_secs(timeout_secs))
        }
        Command::CheckConfig => {
            config.validate_paths()?;
            println!("{}", toml::to_string_pretty(&config)?);
            Ok(())
        }
    }
}

fn init_tracing(format: LogFormat) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,katago_server=info,tower_http=info".into());
    let registry = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Text => registry.with(tracing_subscriber::fmt::layer()).init(),
        LogFormat::Json => registry
            .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
            .init(),
    }
}

fn serve(config: Config) -> anyhow::Result<()> {
    init_tracing(config.server.log_format);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start async runtime")?;
    runtime.block_on(serve_async(config))
}

async fn serve_async(config: Config) -> anyhow::Result<()> {
    info!(version = %*LONG_VERSION, "starting katago-server");
    config.validate_paths()?;

    let metrics = katago_server::metrics::install_global()?;
    katago_server::metrics::spawn_upkeep(metrics.clone());

    let engine = AnalysisEngine::start(config.katago.clone())
        .await
        .context("could not start KataGo")?;

    let bind = config.bind_address();
    let state = AppState {
        engine: engine.clone(),
        config: Arc::new(config),
        metrics,
    };
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("cannot bind {bind}"))?;
    info!("listening on http://{bind} (docs at /docs, health at /api/v1/health)");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    info!("HTTP server stopped, shutting down KataGo");
    engine.shutdown().await;
    info!("bye");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl-c: {e}");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => error!("failed to listen for SIGTERM: {e}"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("received ctrl-c"),
        () = terminate => info!("received SIGTERM"),
    }
}

/// Minimal HTTP/1.1 GET without pulling in an HTTP client.
fn healthcheck(config: &Config, path: &str, timeout: Duration) -> anyhow::Result<()> {
    let host = match config.server.host.as_str() {
        "" | "::" | "0.0.0.0" | "[::]" => "localhost",
        other => other.trim_matches(['[', ']']),
    };
    let addrs: Vec<_> = (host, config.server.port)
        .to_socket_addrs()
        .with_context(|| format!("cannot resolve {host}"))?
        .collect();
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return probe(stream, host, path, timeout),
            Err(e) => last_err = Some(e),
        }
    }
    bail!(
        "cannot connect to {host}:{}: {}",
        config.server.port,
        last_err.map_or_else(|| "no addresses".to_owned(), |e| e.to_string())
    )
}

fn probe(mut stream: TcpStream, host: &str, path: &str, timeout: Duration) -> anyhow::Result<()> {
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: katago-server-healthcheck\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let status_line = response.lines().next().unwrap_or_default();
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .with_context(|| format!("malformed status line {status_line:?}"))?;
    if (200..300).contains(&code) {
        println!("healthy: {status_line}");
        Ok(())
    } else {
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
        bail!("unhealthy: {status_line} {body}")
    }
}
