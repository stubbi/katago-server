//! HTTP handlers.

use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

use super::problem::{ApiError, Json};
use super::types::{
    AnalysisRequest, AnalysisResponse, CacheClearResponse, EngineHealth, GameAnalysisResponse,
    HealthResponse, IndexResponse, KatagoVersionInfo, ModelInfo, ServerVersion, VersionResponse,
};
use super::{AppState, validate};

const ANALYSIS_PATH: &str = "/api/v1/analysis";
const GAME_ANALYSIS_PATH: &str = "/api/v1/analysis/game";

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

/// Service description and links.
#[utoipa::path(
    get,
    path = "/",
    tag = "operations",
    responses((status = 200, description = "Service description", body = IndexResponse))
)]
pub async fn index() -> axum::Json<IndexResponse> {
    axum::Json(IndexResponse {
        name: "katago-server".to_owned(),
        version: crate::VERSION.to_owned(),
        docs: "/docs".to_owned(),
        openapi: "/api/v1/openapi.json".to_owned(),
        health: "/api/v1/health".to_owned(),
    })
}

/// Analyse the final position of a sequence of moves.
#[utoipa::path(
    post,
    path = "/api/v1/analysis",
    tag = "analysis",
    request_body = AnalysisRequest,
    responses(
        (status = 200, description = "Analysis of the final position", body = AnalysisResponse),
        (status = 400, description = "Invalid request", body = super::problem::ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "KataGo unavailable or server overloaded", body = super::problem::ProblemDetails, content_type = "application/problem+json"),
        (status = 504, description = "Analysis timed out", body = super::problem::ProblemDetails, content_type = "application/problem+json"),
    )
)]
pub async fn analysis(
    State(state): State<AppState>,
    Json(request): Json<AnalysisRequest>,
) -> Result<axum::Json<AnalysisResponse>, ApiError> {
    let prepared = validate::build_query(&request, state.engine.config(), false)
        .map_err(|e| e.with_instance(ANALYSIS_PATH))?;
    let client_id = prepared.client_id;
    let attach = |e: ApiError| {
        e.with_instance(ANALYSIS_PATH)
            .with_request_id(client_id.clone())
    };

    let mut results = state
        .engine
        .analyze(&prepared.query)
        .await
        .map_err(|e| attach(ApiError::from(e)))?;
    let mut result = results
        .pop()
        .ok_or_else(|| attach(ApiError::internal("KataGo returned no result")))?;
    if result.no_results {
        return Err(attach(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "analysis-terminated",
            "Analysis Terminated",
            "the search was terminated before it produced a result",
        )));
    }
    result.id = client_id;
    Ok(axum::Json(result))
}

/// Analyse every turn of a game (or the turns listed in `analyzeTurns`) in one query.
#[utoipa::path(
    post,
    path = "/api/v1/analysis/game",
    tag = "analysis",
    request_body = AnalysisRequest,
    responses(
        (status = 200, description = "One analysis per requested turn, ordered by turn number", body = GameAnalysisResponse),
        (status = 400, description = "Invalid request", body = super::problem::ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "KataGo unavailable or server overloaded", body = super::problem::ProblemDetails, content_type = "application/problem+json"),
        (status = 504, description = "Analysis timed out", body = super::problem::ProblemDetails, content_type = "application/problem+json"),
    )
)]
pub async fn analysis_game(
    State(state): State<AppState>,
    Json(request): Json<AnalysisRequest>,
) -> Result<axum::Json<GameAnalysisResponse>, ApiError> {
    let prepared = validate::build_query(&request, state.engine.config(), true)
        .map_err(|e| e.with_instance(GAME_ANALYSIS_PATH))?;
    let client_id = prepared.client_id;
    let attach = |e: ApiError| {
        e.with_instance(GAME_ANALYSIS_PATH)
            .with_request_id(client_id.clone())
    };

    let mut turns = state
        .engine
        .analyze(&prepared.query)
        .await
        .map_err(|e| attach(ApiError::from(e)))?;
    if turns.iter().any(|t| t.no_results) {
        return Err(attach(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "analysis-terminated",
            "Analysis Terminated",
            "the search was terminated before it produced all results",
        )));
    }
    for turn in &mut turns {
        turn.id.clone_from(&client_id);
    }
    Ok(axum::Json(GameAnalysisResponse {
        id: client_id,
        board_x_size: prepared.query.board_x_size,
        board_y_size: prepared.query.board_y_size,
        turns,
    }))
}

/// Detailed health. 200 once KataGo has loaded and answered, 503 otherwise.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "operations",
    responses(
        (status = 200, description = "Ready to serve analysis", body = HealthResponse),
        (status = 503, description = "Starting up, or KataGo is down", body = HealthResponse),
    )
)]
pub async fn health(State(state): State<AppState>) -> Response {
    let status = state.engine.status();
    let text = if status.ready {
        "healthy"
    } else if status.alive {
        "starting"
    } else {
        "unhealthy"
    };
    let code = if status.ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = HealthResponse {
        status: text.to_owned(),
        timestamp: now_rfc3339(),
        uptime: status.uptime.as_secs(),
        katago: EngineHealth {
            alive: status.alive,
            ready: status.ready,
            restarts: status.restarts,
            version: status.version.map(|v| v.version),
        },
    };
    (code, axum::Json(body)).into_response()
}

/// Liveness: 200 unless KataGo is down and the restart budget is exhausted.
#[utoipa::path(
    get,
    path = "/api/v1/health/live",
    tag = "operations",
    responses(
        (status = 200, description = "Process is live"),
        (status = 503, description = "KataGo is down for good; restart the server"),
    )
)]
pub async fn health_live(State(state): State<AppState>) -> Response {
    let status = state.engine.status();
    let can_recover = status.alive || status.restarts < status.max_restart_attempts;
    let code = if can_recover {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        axum::Json(serde_json::json!({ "status": if can_recover { "live" } else { "dead" } })),
    )
        .into_response()
}

/// Readiness: 200 once KataGo has loaded its network and answered a query.
#[utoipa::path(
    get,
    path = "/api/v1/health/ready",
    tag = "operations",
    responses(
        (status = 200, description = "Ready"),
        (status = 503, description = "Not ready"),
    )
)]
pub async fn health_ready(State(state): State<AppState>) -> Response {
    let ready = state.engine.status().ready;
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        axum::Json(serde_json::json!({ "status": if ready { "ready" } else { "not-ready" } })),
    )
        .into_response()
}

/// Server, KataGo and model versions. Never blocks on KataGo.
#[utoipa::path(
    get,
    path = "/api/v1/version",
    tag = "operations",
    responses((status = 200, description = "Version information", body = VersionResponse))
)]
pub async fn version(State(state): State<AppState>) -> axum::Json<VersionResponse> {
    let katago_config = state.engine.config();
    let katago = state.engine.status().version.map(|v| KatagoVersionInfo {
        version: v.version,
        git_hash: v.git_hash,
    });
    axum::Json(VersionResponse {
        server: ServerVersion {
            name: "katago-server".to_owned(),
            version: crate::VERSION.to_owned(),
            git_sha: crate::GIT_SHA.map(ToOwned::to_owned),
        },
        katago,
        model: ModelInfo {
            name: file_name(&katago_config.model_path),
            human_model: katago_config.human_model_path.as_deref().map(file_name),
        },
    })
}

/// Clear KataGo's neural network cache.
#[utoipa::path(
    post,
    path = "/api/v1/cache/clear",
    tag = "operations",
    responses(
        (status = 200, description = "Cache cleared", body = CacheClearResponse),
        (status = 503, description = "KataGo unavailable", body = super::problem::ProblemDetails, content_type = "application/problem+json"),
    )
)]
pub async fn cache_clear(
    State(state): State<AppState>,
) -> Result<axum::Json<CacheClearResponse>, ApiError> {
    state
        .engine
        .clear_cache()
        .await
        .map_err(|e| ApiError::from(e).with_instance("/api/v1/cache/clear"))?;
    Ok(axum::Json(CacheClearResponse {
        status: "cleared".to_owned(),
        timestamp: now_rfc3339(),
    }))
}

/// Prometheus metrics in text exposition format.
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "operations",
    responses((status = 200, description = "Prometheus text format", content_type = "text/plain"))
)]
pub async fn metrics(State(state): State<AppState>) -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        state.metrics.render(),
    )
        .into_response()
}

/// The OpenAPI document as JSON.
pub async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    use utoipa::OpenApi as _;
    axum::Json(super::openapi::ApiDoc::openapi())
}

/// 404 as problem details.
pub async fn not_found(uri: Uri) -> ApiError {
    ApiError::not_found(uri.path())
}

/// 405 as problem details.
pub async fn method_not_allowed(uri: Uri) -> ApiError {
    ApiError::method_not_allowed().with_instance(uri.path())
}
