//! OpenAPI document generated from the handler and type annotations.

use utoipa::OpenApi;

use super::{handlers, problem, types};

/// The OpenAPI 3.1 description of this server.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "KataGo Server",
        description = "REST API in front of the KataGo Go engine: position and whole-game analysis, \
                       ownership, policy and Human SL profiles. All errors are RFC 9457 problem details.",
        license(name = "MIT", url = "https://github.com/goban-app/katago-server/blob/main/LICENSE"),
        contact(name = "goban-app", url = "https://github.com/goban-app/katago-server"),
    ),
    paths(
        handlers::index,
        handlers::analysis,
        handlers::analysis_game,
        handlers::health,
        handlers::health_live,
        handlers::health_ready,
        handlers::version,
        handlers::cache_clear,
        handlers::metrics,
    ),
    components(schemas(
        types::AnalysisRequest,
        types::MoveInput,
        types::Rules,
        types::MoveFilter,
        types::AnalysisResponse,
        types::GameAnalysisResponse,
        types::MoveInfo,
        types::RootInfo,
        types::HealthResponse,
        types::EngineHealth,
        types::VersionResponse,
        types::ServerVersion,
        types::KatagoVersionInfo,
        types::ModelInfo,
        types::CacheClearResponse,
        types::IndexResponse,
        problem::ProblemDetails,
    )),
    tags(
        (name = "analysis", description = "Position and game analysis"),
        (name = "operations", description = "Health, version, cache and metrics"),
    )
)]
#[derive(Debug)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_lists_every_route() {
        let doc = ApiDoc::openapi();
        let paths: Vec<&String> = doc.paths.paths.keys().collect();
        for expected in [
            "/",
            "/api/v1/analysis",
            "/api/v1/analysis/game",
            "/api/v1/health",
            "/api/v1/health/live",
            "/api/v1/health/ready",
            "/api/v1/version",
            "/api/v1/cache/clear",
            "/metrics",
        ] {
            assert!(
                paths.iter().any(|p| *p == expected),
                "missing {expected} in {paths:?}"
            );
        }
        assert_eq!(doc.info.version, crate::VERSION);
        let json = doc.to_json().unwrap();
        assert!(json.contains("ProblemDetails"));
    }
}
