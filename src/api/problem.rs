//! RFC 9457 / RFC 7807 problem details for every error the API returns.

use axum::extract::rejection::JsonRejection;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tracing::{debug, warn};
use utoipa::ToSchema;

use crate::error::EngineError;

/// Base URI for problem `type` values; the fragment names the problem.
pub const PROBLEM_TYPE_BASE: &str =
    "https://github.com/goban-app/katago-server/blob/main/docs/problems.md#";

/// Media type for problem responses.
pub const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";

/// The JSON body of an error response.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    /// URI identifying the problem type.
    #[serde(rename = "type")]
    #[schema(
        example = "https://github.com/goban-app/katago-server/blob/main/docs/problems.md#invalid-request"
    )]
    pub problem_type: String,
    /// Short human-readable summary.
    #[schema(example = "Invalid Request")]
    pub title: String,
    /// HTTP status code.
    #[schema(example = 400)]
    pub status: u16,
    /// Explanation specific to this occurrence.
    #[schema(example = "move 3 (\"Z99\") is not on a 19x19 board")]
    pub detail: String,
    /// The request path, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// The `requestId` from the request body, when one was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// The request field the problem relates to, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// An error that renders as a problem-details response.
#[derive(Debug, Clone)]
pub struct ApiError {
    status: StatusCode,
    slug: &'static str,
    title: &'static str,
    detail: String,
    instance: Option<String>,
    request_id: Option<String>,
    field: Option<String>,
}

impl ApiError {
    /// Creates an error with an explicit status, type slug and title.
    pub fn new(
        status: StatusCode,
        slug: &'static str,
        title: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status,
            slug,
            title,
            detail: detail.into(),
            instance: None,
            request_id: None,
            field: None,
        }
    }

    /// 400: the request is well-formed JSON but semantically wrong.
    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid-request",
            "Invalid Request",
            detail,
        )
    }

    /// 400: the body is not valid JSON.
    pub fn malformed_json(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "malformed-json",
            "Malformed JSON",
            detail,
        )
    }

    /// 404.
    pub fn not_found(path: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not-found",
            "Not Found",
            format!("no route for {path}"),
        )
        .with_instance(path)
    }

    /// 405.
    pub fn method_not_allowed() -> Self {
        Self::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method-not-allowed",
            "Method Not Allowed",
            "this route does not support the request method",
        )
    }

    /// 413.
    pub fn payload_too_large(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload-too-large",
            "Payload Too Large",
            detail,
        )
    }

    /// 415.
    pub fn unsupported_media_type() -> Self {
        Self::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported-media-type",
            "Unsupported Media Type",
            "request body must be JSON with Content-Type: application/json",
        )
    }

    /// 503: too many requests are already being processed.
    pub fn overloaded() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "overloaded",
            "Server Overloaded",
            "too many requests are being processed; retry shortly",
        )
    }

    /// 504: the whole request exceeded the server's request timeout.
    pub fn request_timeout(secs: u64) -> Self {
        Self::new(
            StatusCode::GATEWAY_TIMEOUT,
            "request-timeout",
            "Request Timeout",
            format!("the request did not complete within {secs} seconds"),
        )
    }

    /// 500.
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal-error",
            "Internal Server Error",
            detail,
        )
    }

    /// Names the request field the problem relates to.
    #[must_use]
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    /// Echoes the client's request id.
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Records the request path.
    #[must_use]
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The problem body.
    pub fn to_problem(&self) -> ProblemDetails {
        ProblemDetails {
            problem_type: format!("{PROBLEM_TYPE_BASE}{}", self.slug),
            title: self.title.to_owned(),
            status: self.status.as_u16(),
            detail: self.detail.clone(),
            instance: self.instance.clone(),
            request_id: self.request_id.clone(),
            field: self.field.clone(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.status.as_u16(),
            self.title,
            self.detail
        )
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            warn!(status = self.status.as_u16(), "{}", self.detail);
        } else {
            debug!(status = self.status.as_u16(), "{}", self.detail);
        }
        let body = serde_json::to_vec(&self.to_problem()).unwrap_or_default();
        (
            self.status,
            [(header::CONTENT_TYPE, PROBLEM_CONTENT_TYPE)],
            body,
        )
            .into_response()
    }
}

impl From<EngineError> for ApiError {
    fn from(err: EngineError) -> Self {
        match err {
            EngineError::Timeout(secs) => Self::new(
                StatusCode::GATEWAY_TIMEOUT,
                "analysis-timeout",
                "Analysis Timeout",
                format!(
                    "KataGo did not finish within {secs} seconds; lower maxVisits or raise katago.move_timeout_secs"
                ),
            ),
            EngineError::ProcessDied | EngineError::ProcessStartFailed(_) => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "engine-unavailable",
                "Engine Unavailable",
                "KataGo is not running; it is being restarted, retry shortly",
            ),
            EngineError::ShuttingDown => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "shutting-down",
                "Shutting Down",
                "the server is shutting down",
            ),
            EngineError::Rejected { message, field } => {
                let err = Self::invalid_request(format!("KataGo rejected the request: {message}"));
                match field {
                    Some(f) => err.with_field(f),
                    None => err,
                }
            }
            EngineError::Katago(message) => Self::new(
                StatusCode::BAD_GATEWAY,
                "engine-error",
                "Engine Error",
                format!("KataGo returned an error: {message}"),
            ),
            EngineError::Parse(message) => Self::new(
                StatusCode::BAD_GATEWAY,
                "engine-error",
                "Engine Error",
                format!("could not parse KataGo's response: {message}"),
            ),
            EngineError::Io(e) => Self::internal(format!("I/O error: {e}")),
            EngineError::Json(e) => Self::internal(format!("JSON error: {e}")),
        }
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        match rejection {
            JsonRejection::JsonDataError(e) => Self::invalid_request(e.body_text()),
            JsonRejection::JsonSyntaxError(e) => Self::malformed_json(e.body_text()),
            JsonRejection::MissingJsonContentType(_) => Self::unsupported_media_type(),
            JsonRejection::BytesRejection(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                Self::payload_too_large(e.body_text())
            }
            other => Self::new(
                other.status(),
                "invalid-request",
                "Invalid Request",
                other.body_text(),
            ),
        }
    }
}

/// JSON extractor whose rejections are problem-details responses.
#[derive(Debug, Clone, Copy, Default, axum::extract::FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct Json<T>(pub T);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_body_has_rfc_fields_and_extensions() {
        let err = ApiError::invalid_request("bad move")
            .with_field("moves")
            .with_request_id("r-1")
            .with_instance("/api/v1/analysis");
        let problem = err.to_problem();
        let json = serde_json::to_value(&problem).unwrap();
        assert_eq!(json["type"], format!("{PROBLEM_TYPE_BASE}invalid-request"));
        assert_eq!(json["title"], "Invalid Request");
        assert_eq!(json["status"], 400);
        assert_eq!(json["detail"], "bad move");
        assert_eq!(json["field"], "moves");
        assert_eq!(json["requestId"], "r-1");
        assert_eq!(json["instance"], "/api/v1/analysis");
    }

    #[test]
    fn optional_fields_are_omitted() {
        let json = serde_json::to_value(ApiError::overloaded().to_problem()).unwrap();
        assert!(json.get("field").is_none());
        assert!(json.get("requestId").is_none());
        assert!(json.get("instance").is_none());
    }

    #[test]
    fn engine_errors_map_to_sensible_statuses() {
        assert_eq!(
            ApiError::from(EngineError::Timeout(5)).status(),
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            ApiError::from(EngineError::ProcessDied).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let rejected = ApiError::from(EngineError::Rejected {
            message: "Illegal move 1: D4".into(),
            field: Some("moves".into()),
        });
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        assert_eq!(rejected.to_problem().field.as_deref(), Some("moves"));
        assert_eq!(
            ApiError::from(EngineError::Katago("boom".into())).status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ApiError::from(EngineError::ShuttingDown).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn renders_problem_json_content_type() {
        let response = ApiError::not_found("/nope").into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            PROBLEM_CONTENT_TYPE
        );
    }
}
