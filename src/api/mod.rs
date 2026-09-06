//! HTTP API: router, middleware stack and shared state.

pub mod handlers;
pub mod openapi;
pub mod problem;
pub mod types;
pub mod validate;

use std::sync::Arc;
use std::time::Duration;

use axum::error_handling::HandleErrorLayer;
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{HeaderValue, Method, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{BoxError, Router, middleware};
use metrics_exporter_prometheus::PrometheusHandle;
use tower::ServiceBuilder;
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::OpenApi as _;
use utoipa_scalar::{Scalar, Servable as _};

use crate::config::Config;
use crate::engine::AnalysisEngine;
use problem::ApiError;

/// State shared by all handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    /// The supervised KataGo process.
    pub engine: AnalysisEngine,
    /// Effective configuration.
    pub config: Arc<Config>,
    /// Prometheus recorder handle used to render `/metrics`.
    pub metrics: PrometheusHandle,
}

/// Builds the complete application router with its middleware stack.
pub fn build_router(state: AppState) -> Router {
    let server = &state.config.server;
    let request_timeout = Duration::from_secs(server.request_timeout_secs);
    let timeout_secs = server.request_timeout_secs;

    let routes = Router::new()
        .route("/", get(handlers::index))
        .route("/api/v1/analysis", post(handlers::analysis))
        .route("/api/v1/analysis/game", post(handlers::analysis_game))
        .route("/api/v1/health", get(handlers::health))
        .route("/api/v1/health/live", get(handlers::health_live))
        .route("/api/v1/health/ready", get(handlers::health_ready))
        .route("/api/v1/version", get(handlers::version))
        .route("/api/v1/cache/clear", post(handlers::cache_clear))
        .route("/api/v1/openapi.json", get(handlers::openapi_json))
        .route("/metrics", get(handlers::metrics))
        .merge(Scalar::with_url("/docs", openapi::ApiDoc::openapi()))
        .fallback(handlers::not_found)
        .method_not_allowed_fallback(handlers::method_not_allowed);

    routes
        .layer(middleware::from_fn(crate::metrics::track_http))
        .layer(DefaultBodyLimit::max(server.max_body_bytes))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(move |err: BoxError| async move {
                    handle_middleware_error(&err, timeout_secs)
                }))
                .load_shed()
                .layer(GlobalConcurrencyLimitLayer::new(
                    server.max_concurrent_requests,
                ))
                .timeout(request_timeout),
        )
        .layer(cors_layer(&server.cors_allowed_origins))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request| {
                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    tracing::info_span!(
                        "http",
                        method = %request.method(),
                        path = %request.uri().path(),
                        request_id = %request_id,
                    )
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

fn handle_middleware_error(err: &BoxError, timeout_secs: u64) -> Response {
    use axum::response::IntoResponse as _;
    if err.is::<tower::timeout::error::Elapsed>() {
        ApiError::request_timeout(timeout_secs).into_response()
    } else if err.is::<tower::load_shed::error::Overloaded>() {
        ApiError::overloaded().into_response()
    } else {
        ApiError::internal(format!("middleware failure: {err}")).into_response()
    }
}

/// CORS for browser clients. `["*"]` allows any origin; otherwise only the listed ones.
pub fn cors_layer(origins: &[String]) -> CorsLayer {
    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            "x-request-id".parse().expect("valid header"),
        ])
        .expose_headers(["x-request-id"
            .parse::<header::HeaderName>()
            .expect("valid header")])
        .max_age(Duration::from_secs(3600));
    if origins.iter().any(|o| o == "*") {
        base.allow_origin(Any)
    } else {
        let list: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        base.allow_origin(AllowOrigin::list(list))
    }
}
