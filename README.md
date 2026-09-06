# KataGo Server

[![CI](https://github.com/goban-app/katago-server/actions/workflows/ci.yml/badge.svg)](https://github.com/goban-app/katago-server/actions/workflows/ci.yml)
[![Container image](https://img.shields.io/badge/ghcr.io-goban--app%2Fkatago--server-44cc11?logo=docker)](https://github.com/goban-app/katago-server/pkgs/container/katago-server)
[![Helm chart](https://img.shields.io/badge/helm-goban--app.github.io%2Fkatago--server-0f1689?logo=helm)](https://goban-app.github.io/katago-server)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

<p align="center">
  <img src="assets/katago-mascot.png" alt="KataGo Server mascot" width="240">
</p>

A REST API in front of the [KataGo](https://github.com/lightvector/KataGo) Go engine, written in Rust with Axum.
It supervises a `katago analysis` process, validates requests, and exposes position and whole-game analysis
with ownership, policy and Human SL profiles. Errors are RFC 9457 problem details. Ships as Docker images
(CPU and CUDA), a Helm chart, and a single static binary.

## Quick start

```bash
docker run -p 2718:2718 ghcr.io/goban-app/katago-server:latest
```

Wait for `GET /api/v1/health` to return 200 (the network loads first), then:

```bash
curl -s http://localhost:2718/api/v1/analysis \
  -H 'Content-Type: application/json' \
  -d '{"moves": ["D4", "Q16", "R4"], "komi": 7.5, "rules": "chinese", "maxVisits": 50, "includeOwnership": true}'
```

```json
{
  "id": "3f1c…", "turnNumber": 3,
  "moveInfos": [{"moveCoord": "D16", "visits": 21, "winrate": 0.52, "scoreLead": 1.9, "pv": ["D16", "C14"], "...": "..."}],
  "rootInfo": {"winrate": 0.51, "scoreLead": 1.5, "visits": 50, "currentPlayer": "W"},
  "ownership": [0.02, -0.11, "..."]
}
```

Interactive API docs live at `http://localhost:2718/docs`.

## Features

- Position analysis and whole-game analysis (one result per turn) in a single query
- Every KataGo analysis option forwarded: ownership, ownership stdev, per-move ownership, policy, PV visits, move filters, `overrideSettings`, Human SL profiles
- Strict request validation with precise 400 errors before anything reaches KataGo
- Supervised engine: fail-fast on crash, automatic restart with backoff, terminate-on-timeout and on client disconnect
- Liveness and readiness endpoints, Prometheus metrics, text or JSON logs, `x-request-id` propagation
- Configurable CORS, request timeout, concurrency limit and body limit
- OpenAPI 3.1 document generated from the code, served with an interactive UI
- Docker images for CPU (amd64, arm64) and CUDA, Helm chart, systemd unit

## Endpoints

| Method | Path                     | Purpose                                                  |
|--------|--------------------------|----------------------------------------------------------|
| POST   | `/api/v1/analysis`       | Analyse the final position of a move sequence            |
| POST   | `/api/v1/analysis/game`  | Analyse every turn (or `analyzeTurns`) of a game         |
| GET    | `/api/v1/health`         | Detailed health; 200 only once KataGo is ready           |
| GET    | `/api/v1/health/live`    | Liveness probe                                           |
| GET    | `/api/v1/health/ready`   | Readiness probe                                          |
| GET    | `/api/v1/version`        | Server, KataGo and model versions                        |
| POST   | `/api/v1/cache/clear`    | Clear KataGo's neural network cache                      |
| GET    | `/metrics`               | Prometheus metrics                                       |
| GET    | `/api/v1/openapi.json`   | OpenAPI document                                         |
| GET    | `/docs`                  | Interactive API reference                                |

Full reference: [docs/api.md](docs/api.md). Error catalogue: [docs/problems.md](docs/problems.md).

## Configuration

Settings come from built-in defaults, then `config.toml` (`--config` or `KATAGO_CONFIG_FILE`),
then environment variables such as `KATAGO_SERVER_PORT` or `KATAGO_MODEL_PATH`.
`katago-server check-config` prints the effective configuration.
See [docs/configuration.md](docs/configuration.md).

## Deployment

- Docker: `latest` (CPU), `latest-gpu`, `latest-combo-cpu`, `latest-minimal` and more
- Helm: `helm repo add katago-server https://goban-app.github.io/katago-server`
- Bare metal: `katago-server.service` for systemd

See [docs/deployment.md](docs/deployment.md).

## Development

```bash
make test    # unit + end-to-end tests against a fake KataGo (needs python3)
make lint    # fmt, clippy, cargo-deny
make smoke   # test.sh against a running server
```

See [docs/development.md](docs/development.md), [CONTRIBUTING.md](CONTRIBUTING.md) and [RELEASING.md](RELEASING.md).

## License

MIT. See [LICENSE](LICENSE). KataGo and its networks have their own licenses.

## Links

- [KataGo](https://github.com/lightvector/KataGo) and the [Analysis Engine protocol](https://github.com/lightvector/KataGo/blob/master/docs/Analysis_Engine.md)
- [KataGo networks](https://katagotraining.org/networks/)
- [Changelog](CHANGELOG.md) · [Security policy](SECURITY.md) · [Client snippets](docs/clients.md)
