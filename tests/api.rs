//! End-to-end tests of the HTTP API against the fake KataGo.

mod common;

use std::time::Duration;

use axum::http::{StatusCode, header};
use serde_json::json;

use common::{FakeOptions, TestServer, assert_problem, json, text};

#[tokio::test]
async fn health_goes_from_starting_to_healthy() {
    let server = TestServer::start_with(
        FakeOptions {
            env: &[("FAKE_KATAGO_STARTUP_DELAY", "1.5")],
        },
        |_| {},
    )
    .await;

    let response = server.get("/api/v1/health").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = json(response).await;
    assert_eq!(body["status"], "starting");
    assert_eq!(body["katago"]["alive"], true);
    assert_eq!(body["katago"]["ready"], false);

    assert_eq!(
        server.get("/api/v1/health/ready").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        server.get("/api/v1/health/live").await.status(),
        StatusCode::OK,
        "live while starting"
    );

    server.wait_ready().await;
    let response = server.get("/api/v1/health").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["katago"]["ready"], true);
    assert_eq!(body["katago"]["restarts"], 0);
    assert_eq!(body["katago"]["version"], "9.9.9-fake");
    assert!(body["uptime"].is_u64());
    assert!(body["timestamp"].as_str().unwrap().ends_with('Z'));
    assert_eq!(
        server.get("/api/v1/health/ready").await.status(),
        StatusCode::OK
    );
    server.engine.shutdown().await;
}

#[tokio::test]
async fn version_reports_real_katago_version() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    let body = json(server.get("/api/v1/version").await).await;
    assert_eq!(body["server"]["name"], "katago-server");
    assert_eq!(body["server"]["version"], katago_server::VERSION);
    assert_eq!(body["katago"]["version"], "9.9.9-fake");
    assert_eq!(body["katago"]["gitHash"], "fakehash123");
    assert_eq!(body["model"]["name"], "model.bin.gz");
    assert!(body["model"].get("humanModel").is_none());
    server.engine.shutdown().await;
}

#[tokio::test]
async fn analysis_forwards_every_field_and_echoes_request_id() {
    let server = TestServer::start().await;
    server.wait_ready().await;

    let response = server
        .post_json(
            "/api/v1/analysis",
            &json!({
                "requestId": "game-7",
                "moves": ["d4", "Q16", "pass"],
                "rules": {"ko": "SIMPLE", "scoring": "AREA"},
                "komi": 7.5,
                "boardXSize": 19,
                "boardYSize": 19,
                "initialPlayer": "b",
                "maxVisits": 42,
                "rootPolicyTemperature": 1.1,
                "rootFpuReductionMax": 0.2,
                "analysisPVLen": 3,
                "includeOwnership": true,
                "includeOwnershipStdev": true,
                "includeMovesOwnership": true,
                "includePolicy": true,
                "includePVVisits": true,
                "avoidMoves": [{"player": "w", "moves": ["c3"], "untilDepth": 2}],
                "allowMoves": [{"player": "B", "moves": ["Q16"], "untilDepth": 1}],
                "overrideSettings": {"humanSLProfile": "rank_5k"},
                "priority": 3
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    assert!(response.headers().contains_key("x-request-id"));
    let body = json(response).await;
    assert_eq!(body["id"], "game-7");
    assert_eq!(body["turnNumber"], 3);
    assert_eq!(body["isDuringSearch"], false);
    assert!(body.get("noResults").is_none());
    assert_eq!(body["moveInfos"][0]["moveCoord"], "Q16");
    assert_eq!(body["moveInfos"][0]["visits"], 42);
    assert_eq!(body["moveInfos"][0]["pvVisits"], json!([42]));
    assert_eq!(
        body["moveInfos"][0]["ownership"].as_array().unwrap().len(),
        361
    );
    assert_eq!(body["rootInfo"]["currentPlayer"], "W");
    assert_eq!(body["ownership"].as_array().unwrap().len(), 361);
    assert_eq!(body["ownershipStdev"].as_array().unwrap().len(), 361);
    assert_eq!(body["policy"].as_array().unwrap().len(), 362);

    let queries = server.queries();
    let query = queries
        .iter()
        .find(|q| q.get("moves").is_some())
        .expect("analysis query reached katago");
    assert_ne!(
        query["id"], "game-7",
        "client ids never reach KataGo verbatim"
    );
    assert_eq!(
        query["moves"],
        json!([["B", "D4"], ["W", "Q16"], ["B", "pass"]])
    );
    assert_eq!(query["rules"]["ko"], "SIMPLE");
    assert_eq!(query["initialPlayer"], "B");
    assert_eq!(query["maxVisits"], 42);
    assert_eq!(query["rootPolicyTemperature"], 1.1);
    assert_eq!(query["rootFpuReductionMax"], 0.2);
    assert_eq!(query["analysisPVLen"], 3);
    assert_eq!(query["includePVVisits"], true);
    assert_eq!(query["includeOwnershipStdev"], true);
    assert_eq!(query["includeMovesOwnership"], true);
    assert_eq!(query["avoidMoves"][0]["player"], "W");
    assert_eq!(query["avoidMoves"][0]["moves"], json!(["C3"]));
    assert_eq!(query["avoidMoves"][0]["untilDepth"], 2);
    assert_eq!(query["allowMoves"][0]["moves"], json!(["Q16"]));
    assert_eq!(query["overrideSettings"]["humanSLProfile"], "rank_5k");
    assert_eq!(query["priority"], 3);
    assert!(query.get("analyzeTurns").is_none());
    server.engine.shutdown().await;
}

#[tokio::test]
async fn analysis_uses_defaults_for_minimal_request() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    let body = json(
        server
            .post_json("/api/v1/analysis", &json!({"moves": []}))
            .await,
    )
    .await;
    assert!(uuid_like(body["id"].as_str().unwrap()));
    assert_eq!(body["turnNumber"], 0);
    assert_eq!(body["rootInfo"]["currentPlayer"], "B");
    let query = server
        .queries()
        .into_iter()
        .find(|q| q.get("moves").is_some())
        .unwrap();
    assert_eq!(query["komi"], 7.5);
    assert_eq!(query["rules"], "chinese");
    assert_eq!(query["maxVisits"], 10);
    assert_eq!(query["boardXSize"], 19);
    server.engine.shutdown().await;
}

fn uuid_like(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
}

#[tokio::test]
async fn game_analysis_returns_one_result_per_turn_in_order() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    let response = server
        .post_json(
            "/api/v1/analysis/game",
            &json!({"requestId": "review-1", "moves": ["D4", "Q16", "R4"], "boardXSize": 19, "boardYSize": 19}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["id"], "review-1");
    assert_eq!(body["boardXSize"], 19);
    let turns = body["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 4);
    for (i, turn) in turns.iter().enumerate() {
        assert_eq!(turn["turnNumber"], i);
        assert_eq!(turn["id"], "review-1");
        assert!(turn["rootInfo"].is_object());
    }
    assert_eq!(turns[0]["rootInfo"]["currentPlayer"], "B");
    assert_eq!(turns[1]["rootInfo"]["currentPlayer"], "W");

    let response = server
        .post_json(
            "/api/v1/analysis/game",
            &json!({"moves": ["D4", "Q16", "R4"], "analyzeTurns": [3, 1]}),
        )
        .await;
    let body = json(response).await;
    let numbers: Vec<u64> = body["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["turnNumber"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, vec![1, 3]);
    server.engine.shutdown().await;
}

#[tokio::test]
async fn validation_errors_are_problem_json_with_field() {
    let server = TestServer::start().await;
    server.wait_ready().await;

    let body = assert_problem(
        server
            .post_json("/api/v1/analysis", &json!({"moves": ["D4", "Z99"]}))
            .await,
        StatusCode::BAD_REQUEST,
        "invalid-request",
    )
    .await;
    assert_eq!(body["field"], "moves");
    assert_eq!(body["instance"], "/api/v1/analysis");
    assert!(body["detail"].as_str().unwrap().contains("moves[1]"));

    let body = assert_problem(
        server
            .post_json("/api/v1/analysis", &json!({"moves": ["D4", ["W", "Q16"]]}))
            .await,
        StatusCode::BAD_REQUEST,
        "invalid-request",
    )
    .await;
    assert_eq!(body["field"], "moves");

    let body = assert_problem(
        server
            .post_json(
                "/api/v1/analysis",
                &json!({"requestId": "x", "boardXSize": 30}),
            )
            .await,
        StatusCode::BAD_REQUEST,
        "invalid-request",
    )
    .await;
    assert_eq!(body["field"], "boardXSize");

    assert!(
        server.queries().iter().all(|q| q.get("moves").is_none()),
        "invalid requests never reach KataGo"
    );
    server.engine.shutdown().await;
}

#[tokio::test]
async fn katago_rejections_become_400_with_field_and_request_id() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    let body = assert_problem(
        server
            .post_json(
                "/api/v1/analysis",
                &json!({"requestId": "dup", "moves": ["D4", "Q16", "D4"]}),
            )
            .await,
        StatusCode::BAD_REQUEST,
        "invalid-request",
    )
    .await;
    assert_eq!(body["field"], "moves");
    assert_eq!(body["requestId"], "dup");
    assert!(body["detail"].as_str().unwrap().contains("Illegal move"));

    let body = assert_problem(
        server
            .post_json("/api/v1/analysis", &json!({"rules": "klingon"}))
            .await,
        StatusCode::BAD_REQUEST,
        "invalid-request",
    )
    .await;
    assert_eq!(body["field"], "rules");
    server.engine.shutdown().await;
}

#[tokio::test]
async fn katago_internal_errors_become_502_and_warnings_are_tolerated() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    assert_problem(
        server
            .post_json(
                "/api/v1/analysis",
                &json!({"overrideSettings": {"fakeError": "GPU on fire"}}),
            )
            .await,
        StatusCode::BAD_GATEWAY,
        "engine-error",
    )
    .await;
    let response = server
        .post_json(
            "/api/v1/analysis",
            &json!({"overrideSettings": {"fakeWarn": 1}}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "warnings do not fail the request"
    );
    server.engine.shutdown().await;
}

#[tokio::test]
async fn malformed_bodies_and_unknown_routes_are_problem_json() {
    let server = TestServer::start().await;

    assert_problem(
        server
            .post_raw("/api/v1/analysis", "application/json", "{not json".into())
            .await,
        StatusCode::BAD_REQUEST,
        "malformed-json",
    )
    .await;

    let body = assert_problem(
        server
            .post_raw(
                "/api/v1/analysis",
                "application/json",
                r#"{"moves": "D4"}"#.into(),
            )
            .await,
        StatusCode::BAD_REQUEST,
        "invalid-request",
    )
    .await;
    assert!(body["detail"].as_str().unwrap().contains("moves"));

    assert_problem(
        server
            .post_raw("/api/v1/analysis", "text/plain", "{}".into())
            .await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported-media-type",
    )
    .await;

    let body = assert_problem(
        server.get("/api/v2/nope").await,
        StatusCode::NOT_FOUND,
        "not-found",
    )
    .await;
    assert_eq!(body["instance"], "/api/v2/nope");

    assert_problem(
        server.get("/api/v1/analysis").await,
        StatusCode::METHOD_NOT_ALLOWED,
        "method-not-allowed",
    )
    .await;
    server.engine.shutdown().await;
}

#[tokio::test]
async fn oversized_bodies_are_rejected_with_413() {
    let server = TestServer::start_with(FakeOptions::default(), |c| {
        c.server.max_body_bytes = 1024;
    })
    .await;
    let padding = "x".repeat(4096);
    assert_problem(
        server
            .post_json("/api/v1/analysis", &json!({"requestId": padding}))
            .await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload-too-large",
    )
    .await;
    server.engine.shutdown().await;
}

#[tokio::test]
async fn slow_analysis_times_out_with_504_and_is_terminated() {
    let server = TestServer::start_with(FakeOptions::default(), |c| {
        c.katago.move_timeout_secs = 1;
    })
    .await;
    server.wait_ready().await;
    let started = std::time::Instant::now();
    let body = assert_problem(
        server
            .post_json(
                "/api/v1/analysis",
                &json!({"requestId": "slow", "overrideSettings": {"fakeDelayMs": 4000}}),
            )
            .await,
        StatusCode::GATEWAY_TIMEOUT,
        "analysis-timeout",
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(body["requestId"], "slow");

    // The fake answers the delayed query eventually; the terminate must have been sent.
    tokio::time::sleep(Duration::from_millis(3500)).await;
    let queries = server.queries();
    let analysis_id = queries
        .iter()
        .find(|q| q.get("moves").is_some())
        .and_then(|q| q["id"].as_str())
        .unwrap()
        .to_owned();
    assert!(
        queries
            .iter()
            .any(|q| q["action"] == "terminate" && q["terminateId"] == analysis_id),
        "terminate was sent for the timed-out query: {queries:?}"
    );
    // ...and the late answer did not confuse anything.
    let response = server
        .post_json("/api/v1/analysis", &json!({"moves": ["D4"]}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    server.engine.shutdown().await;
}

#[tokio::test]
async fn whole_request_timeout_is_a_504_problem() {
    let server = TestServer::start_with(FakeOptions::default(), |c| {
        c.katago.move_timeout_secs = 1;
        c.server.request_timeout_secs = 1;
    })
    .await;
    server.wait_ready().await;
    // Game analysis with 3 turns, each delayed 600ms: the per-result timeout (1s)
    // never fires but the whole request exceeds the 1s request timeout.
    let response = server
        .post_json(
            "/api/v1/analysis/game",
            &json!({"moves": ["D4", "Q16"], "overrideSettings": {"fakeDelayMs": 600}}),
        )
        .await;
    assert_problem(response, StatusCode::GATEWAY_TIMEOUT, "request-timeout").await;

    // The abandoned query must be terminated inside KataGo once the handler is dropped.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let queries = server.queries();
    let analysis_id = queries
        .iter()
        .find(|q| q.get("moves").is_some())
        .and_then(|q| q["id"].as_str())
        .unwrap()
        .to_owned();
    assert!(
        queries
            .iter()
            .any(|q| q["action"] == "terminate" && q["terminateId"] == analysis_id),
        "terminate was sent for the abandoned query: {queries:?}"
    );
    server.engine.shutdown().await;
}

#[tokio::test]
async fn crash_fails_fast_and_engine_restarts() {
    let server = TestServer::start().await;
    server.wait_ready().await;

    let started = std::time::Instant::now();
    let body = assert_problem(
        server
            .post_json(
                "/api/v1/analysis",
                &json!({"overrideSettings": {"fakeCrash": 1}}),
            )
            .await,
        StatusCode::SERVICE_UNAVAILABLE,
        "engine-unavailable",
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "waiters fail immediately on exit, not after the timeout"
    );
    assert!(body["detail"].as_str().unwrap().contains("restarted"));

    // While down: health unhealthy, live still 200 (restart budget remains).
    let response = server.get("/api/v1/health").await;
    let status = response.status();
    let health = json(response).await;
    if health["katago"]["alive"] == false {
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(health["status"], "unhealthy");
    }
    assert_eq!(
        server.get("/api/v1/health/live").await.status(),
        StatusCode::OK
    );

    server.wait_ready().await;
    let health = json(server.get("/api/v1/health").await).await;
    assert_eq!(health["status"], "healthy");
    assert_eq!(health["katago"]["restarts"], 1);

    let response = server
        .post_json("/api/v1/analysis", &json!({"moves": ["D4"]}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    server.engine.shutdown().await;
}

#[tokio::test]
async fn restart_budget_exhaustion_makes_liveness_fail() {
    let server = TestServer::start_with(FakeOptions::default(), |c| {
        c.katago.max_restart_attempts = 0;
    })
    .await;
    server.wait_ready().await;
    server
        .post_json(
            "/api/v1/analysis",
            &json!({"overrideSettings": {"fakeCrash": 1}}),
        )
        .await;
    server.wait_dead().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        server.get("/api/v1/health/live").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_problem(
        server.post_json("/api/v1/analysis", &json!({})).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "engine-unavailable",
    )
    .await;
    server.engine.shutdown().await;
}

#[tokio::test]
async fn cache_clear_waits_for_acknowledgement() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    let response = server.post_json("/api/v1/cache/clear", &json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["status"], "cleared");
    assert!(
        server
            .queries()
            .iter()
            .any(|q| q["action"] == "clear_cache")
    );
    server.engine.shutdown().await;
}

#[tokio::test]
async fn shutdown_terminates_katago_and_rejects_new_work() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    server.engine.shutdown().await;
    assert!(!server.engine.status().alive);
    assert!(
        server
            .queries()
            .iter()
            .any(|q| q["action"] == "terminate_all")
    );
    assert_problem(
        server.post_json("/api/v1/analysis", &json!({})).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "shutting-down",
    )
    .await;
    assert_eq!(
        server.get("/api/v1/health").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn concurrency_limit_sheds_load_with_503() {
    let server = TestServer::start_with(FakeOptions::default(), |c| {
        c.server.max_concurrent_requests = 1;
    })
    .await;
    server.wait_ready().await;
    let slow = json!({"overrideSettings": {"fakeDelayMs": 1500}});
    let first = server.post_json("/api/v1/analysis", &slow);
    let second = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        server.post_json("/api/v1/health", &json!({})).await
    };
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.status(), StatusCode::OK);
    assert_problem(second, StatusCode::SERVICE_UNAVAILABLE, "overloaded").await;
    server.engine.shutdown().await;
}

#[tokio::test]
async fn metrics_and_docs_are_served() {
    let server = TestServer::start().await;
    server.wait_ready().await;
    server.post_json("/api/v1/analysis", &json!({})).await;

    let response = server.get("/metrics").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    let body = text(response).await;
    if !body.is_empty() {
        assert!(body.contains("http_requests_total"), "{body}");
        assert!(body.contains("katago_engine_up"), "{body}");
    }

    let response = server.get("/api/v1/openapi.json").await;
    assert_eq!(response.status(), StatusCode::OK);
    let doc = json(response).await;
    assert_eq!(doc["info"]["title"], "KataGo Server");
    assert!(doc["paths"]["/api/v1/analysis/game"].is_object());
    assert!(doc["components"]["schemas"]["ProblemDetails"].is_object());

    let response = server.get("/docs").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(text(response).await.contains("<html"));

    let index = json(server.get("/").await).await;
    assert_eq!(index["name"], "katago-server");
    assert_eq!(index["docs"], "/docs");
    server.engine.shutdown().await;
}

#[tokio::test]
async fn cors_reflects_configured_origins() {
    let server = TestServer::start_with(FakeOptions::default(), |c| {
        c.server.cors_allowed_origins = vec!["https://goban.app".into()];
    })
    .await;
    let request = axum::http::Request::builder()
        .method("OPTIONS")
        .uri("/api/v1/analysis")
        .header(header::ORIGIN, "https://goban.app")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(server.app.clone(), request)
        .await
        .unwrap();
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "https://goban.app"
    );
    let request = axum::http::Request::builder()
        .method("OPTIONS")
        .uri("/api/v1/analysis")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(server.app.clone(), request)
        .await
        .unwrap();
    assert!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    server.engine.shutdown().await;
}

#[tokio::test]
async fn missing_binary_fails_startup_with_clear_error() {
    let config = katago_server::config::KatagoConfig {
        katago_path: "/definitely/not/katago".into(),
        ..Default::default()
    };
    let Err(err) = katago_server::engine::AnalysisEngine::start(config).await else {
        panic!("start must fail");
    };
    let text = err.to_string();
    assert!(text.contains("failed to start KataGo"), "{text}");
    assert!(text.contains("/definitely/not/katago"), "{text}");
}

#[tokio::test]
async fn crash_on_start_is_reported_as_unhealthy_then_restarted() {
    let dir = tempfile::TempDir::new().unwrap();
    // First start crashes; the restart (same wrapper) also crashes, so the engine
    // ends up dead with the budget spent. Liveness must then be 503.
    let server = TestServer::start_with(
        FakeOptions {
            env: &[("FAKE_KATAGO_CRASH_ON_START", "1")],
        },
        |c| c.katago.max_restart_attempts = 1,
    )
    .await;
    drop(dir);
    server.wait_dead().await;
    // Give the supervisor time to attempt (and fail) its single restart.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while server.engine.status().restarts < 1 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(server.engine.status().restarts, 1);
    server.wait_dead().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        server.get("/api/v1/health/live").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    server.engine.shutdown().await;
}
