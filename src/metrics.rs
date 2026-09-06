//! Prometheus metrics: an HTTP middleware plus engine counters recorded through
//! the `metrics` facade.

use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

/// Histogram buckets for request durations, in seconds.
const DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0,
];

fn builder() -> PrometheusBuilder {
    PrometheusBuilder::new()
        .set_buckets_for_metric(Matcher::Suffix("duration_seconds".into()), DURATION_BUCKETS)
        .expect("duration buckets are non-empty")
}

/// Installs the process-wide recorder. Fails if one is already installed.
pub fn install_global() -> anyhow::Result<PrometheusHandle> {
    let handle = builder().install_recorder()?;
    describe();
    Ok(handle)
}

/// Builds a recorder that is *not* installed globally. Useful in tests, where
/// several servers live in one process; metrics macros then record nothing.
pub fn detached_handle() -> PrometheusHandle {
    builder().build_recorder().handle()
}

fn describe() {
    metrics::describe_counter!(
        "http_requests_total",
        "HTTP requests handled, by method, matched route and status code"
    );
    metrics::describe_histogram!(
        "http_request_duration_seconds",
        metrics::Unit::Seconds,
        "HTTP request latency, by method and matched route"
    );
    metrics::describe_gauge!(
        "http_requests_in_flight",
        "HTTP requests currently being handled"
    );
    metrics::describe_counter!(
        "katago_analysis_requests_total",
        "Analysis queries sent to KataGo, by outcome"
    );
    metrics::describe_histogram!(
        "katago_analysis_duration_seconds",
        metrics::Unit::Seconds,
        "Time KataGo took to answer an analysis query, by outcome"
    );
    metrics::describe_gauge!("katago_engine_up", "1 while the KataGo process is running");
    metrics::describe_counter!(
        "katago_engine_restarts_total",
        "Times the KataGo process was restarted"
    );
}

/// Spawns a task that periodically performs recorder housekeeping.
pub fn spawn_upkeep(handle: PrometheusHandle) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            handle.run_upkeep();
        }
    });
}

/// Axum middleware recording per-route request counts, latencies and in-flight gauge.
pub async fn track_http(request: Request, next: Next) -> Response {
    let method = request.method().as_str().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_owned(), |p| p.as_str().to_owned());
    let started = Instant::now();
    metrics::gauge!("http_requests_in_flight").increment(1.0);

    let response = next.run(request).await;

    metrics::gauge!("http_requests_in_flight").decrement(1.0);
    let status = response.status().as_u16().to_string();
    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "route" => route.clone(),
        "status" => status
    )
    .increment(1);
    metrics::histogram!("http_request_duration_seconds", "method" => method, "route" => route)
        .record(started.elapsed().as_secs_f64());
    response
}
