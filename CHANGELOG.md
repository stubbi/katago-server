# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.8.0] - 2026-09-07

A ground-up overhaul of the server, its packaging and its documentation.

### Added

- `POST /api/v1/analysis/game`: analyse every turn of a game (or `analyzeTurns`) in one KataGo query.
- `GET /metrics` with Prometheus metrics for HTTP and engine activity.
- `GET /docs` interactive API reference and `GET /api/v1/openapi.json`, generated from the code.
- `GET /api/v1/health/live` and `GET /api/v1/health/ready` for Kubernetes probes; `/api/v1/health` now reports uptime, restarts and KataGo version.
- Queries are terminated inside KataGo when they time out or the client disconnects.
- JSON log format (`server.log_format` / `KATAGO_SERVER_LOG_FORMAT`) and `x-request-id` propagation.
- Configurable CORS origins, request timeout, concurrency limit (load shedding) and body limit.
- `--config FILE` and `KATAGO_CONFIG_FILE` to locate `config.toml`; `katago.default_max_visits`, `katago.max_visits_limit`, `katago.max_restart_attempts`.
- `katago-server healthcheck` (used by the Docker `HEALTHCHECK`) and `katago-server check-config` subcommands.
- Request fields now forwarded to KataGo: `initialPlayer`, `analyzeTurns`, `includeOwnershipStdev`, `includeMovesOwnership`, `rootPolicyTemperature`, `rootFpuReductionMax`, `analysisPVLen`, `avoidMoves`, `allowMoves`, `priority`; rules may be a KataGo rules object.
- Additional response fields from KataGo (`scoreSelfplay`, `edgeVisits`, `weight`, `playSelectionValue`, `pvEdgeVisits`, `rawLead`, `symHash`, `thisHash`, human SL fields, ...).
- Docker images built on KataGo v1.18.2; `GIT_SHA` embedded and reported by `/api/v1/version`.
- cargo-deny, Dependabot, hadolint, shellcheck, helm lint, MSRV and version-consistency checks in CI.
- End-to-end test suite against a fake KataGo (`tests/fake_katago.py`).

### Changed

- `GET /api/v1/version` reports the real KataGo version and git hash instead of a hard-coded string.
- `analysisPVLen` and `includePVVisits` now use KataGo's spelling (the old `analysisPvLen`/`includePvVisits` still work as aliases).
- `/api/v1/health` returns 503 until KataGo has loaded its network and answered a query.
- Docker images run as UID 1000, log at `info`, and no longer need `wget`.
- Problem `type` URLs now point at `docs/problems.md`; problems may carry a `field`.
- Cargo version now matches the released chart version (1.8.0); the engine runs on `tokio::process`.
- Edition 2024, `thiserror` 2, dependency updates resolving two advisories.

### Fixed

- A request mixing bare and `[colour, coordinate]` moves no longer aborts the whole server.
- Invalid coordinates, colours, board sizes, komi and visit counts return 400 instead of being sent to KataGo.
- In-flight requests fail immediately with 503 when KataGo exits, instead of waiting for the timeout.
- `POST /api/v1/cache/clear` waits for KataGo's acknowledgement.
- Two clients reusing the same `requestId` no longer collide inside the engine.
- Helm `config.customConfig` is applied (`KATAGO_CONFIG_FILE`); the old `KATAGO_SERVER__*` names were never read.
- Malformed JSON, wrong content type, 404 and 405 responses are `application/problem+json`.
- Environment variables with unparseable values are reported instead of silently ignored.

### Removed

- The unused GTP bot module and its `regex` and `hyper` direct dependencies.
- `EXAMPLES.md` (see `docs/clients.md`); the `reportDuringSearchEvery` request field, which was never honoured.

[Unreleased]: https://github.com/goban-app/katago-server/compare/v1.8.0...HEAD
[1.8.0]: https://github.com/goban-app/katago-server/compare/v1.7.4...v1.8.0
