# Design: the 1.8.0 overhaul

Date: 2026-09-06. Status: implemented in 1.8.0.

## Goal

Turn katago-server into a production-grade, fully working REST front end for the
KataGo analysis engine: correct under every documented input, observable,
hardened, documented, and verified end to end.

## Constraints

- Keep the existing `/api/v1` request and response contract backward compatible.
- Keep Axum and the KataGo analysis-engine JSON protocol over stdin/stdout.
- Remove dead code rather than preserve it (the GTP bot, unused config types).

## Architecture

```
main.rs          CLI (serve | healthcheck | check-config), tracing, graceful shutdown
lib.rs           module tree so integration tests can build the router in-process
config.rs        Config types, TOML + env loading with an injectable env lookup
coords.rs        GTP coordinate parsing and validation
error.rs         EngineError
engine/mod.rs    AnalysisEngine: tokio::process supervisor, id-routed responses,
                 restart loop with backoff, terminate-on-drop, shutdown
engine/protocol.rs  Query wire type
api/mod.rs       router and middleware stack
api/types.rs     public types, also the parse target for KataGo output
api/validate.rs  AnalysisRequest -> Query; every 400 rule
api/handlers.rs  handlers
api/problem.rs   RFC 9457 ApiError, problem-json Json extractor
api/openapi.rs   utoipa document
metrics.rs       Prometheus recorder and HTTP middleware
```

## Engine

- One `katago analysis` child per server. A reader task parses each stdout line
  as JSON and routes it by `id` to an unbounded channel in a `pending` map, so a
  query can yield several results (one per `analyzeTurns` entry).
- On EOF the process is marked dead, `pending` is cleared (waiters see
  `ProcessDied` immediately), the last 40 stderr lines are logged, and the
  supervisor restarts KataGo with exponential backoff up to
  `max_restart_attempts`.
- Readiness is established by the first successful `query_version`; its answer
  is cached and reported by `/api/v1/version` and `/api/v1/health`.
- `move_timeout_secs` is an inactivity timeout between results. On expiry, or
  when the request future is dropped (HTTP timeout, client disconnect), the
  query is terminated inside KataGo.
- Shutdown: drain HTTP, send `terminate_all`, close stdin, wait 5 s, kill.

## API

- `POST /api/v1/analysis`: unchanged shape; every documented field is validated
  and forwarded. Client `requestId` is echoed but never used as the KataGo id.
- `POST /api/v1/analysis/game`: same body, `analyzeTurns` defaults to every
  turn, returns `{id, boardXSize, boardYSize, turns[]}`.
- `/api/v1/health` (200 only when ready), `/health/live`, `/health/ready`,
  `/version`, `/cache/clear` (awaits the ack), `/metrics`, `/api/v1/openapi.json`,
  `/docs`, `/`.
- All errors, including JSON rejections, 404 and 405, are problem+json with a
  `type` URL into `docs/problems.md`.

## Hardening

Request id propagation, request timeout, global concurrency limit with load
shedding, body limit, configurable CORS, non-root images, built-in health
check subcommand, panic = unwind so one bad request cannot abort the process.

## Testing

Unit tests per module; `tests/api.rs` drives the real engine against
`tests/fake_katago.py`, a faithful stand-in for the analysis protocol; and a
manual run against KataGo 1.18.2 verified the wire format assumptions
(responses per turn, `warning`/`error` lines with `field`, rules objects,
`noResults` on termination, `logToStderr`).
