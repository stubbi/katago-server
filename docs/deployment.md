# Deployment

## Docker images

Images are published to `ghcr.io/goban-app/katago-server`. Each release `X.Y.Z`
produces `X.Y.Z-<variant>` and `latest-<variant>`; the plain `X.Y.Z` and
`latest` tags are the CPU variant.

| Variant | KataGo | Networks | Platforms | Notes |
|---|---|---|---|---|
| `cpu` (= `latest`) | 1.18.2, Eigen backend | `kata1-b28c512nbt-s12043015936-d5616446734.bin.gz` | amd64, arm64 | Default. |
| `human-cpu` | Eigen | `b18c384nbt-humanv0.bin.gz` only | amd64, arm64 | Human-style analysis only. |
| `combo-cpu` | Eigen | b28 standard + human network | amd64, arm64 | Human SL via `overrideSettings.humanSLProfile`. |
| `gpu` | CUDA 12.4 + cuDNN | b28 | amd64 | Needs NVIDIA Container Toolkit, driver >= 525.60. |
| `human-gpu` | CUDA | human network only | amd64 | |
| `combo-gpu` | CUDA | b28 + human | amd64 | |
| `minimal` | none | none | amd64, arm64 | Mount `/models/katago`, `/models/model.bin.gz`, `/models/analysis_config.cfg`. |
| `base` | none | none | amd64, arm64 | Server binary only (statically linked); set all `KATAGO_*` paths yourself. |

All images run as UID 1000 with `WORKDIR /app`, expose port 2718, log at `info`,
and define a `HEALTHCHECK` that runs `katago-server healthcheck` (a built-in HTTP
probe of `/api/v1/health`, no curl or wget needed). KataGo is built from source
at v1.18.2.

```bash
# GPU
docker run --gpus all -p 2718:2718 ghcr.io/goban-app/katago-server:latest-gpu

# custom network and KataGo config on top of the CPU image
docker run -p 2718:2718 \
  -v "$PWD/my-net.bin.gz:/models/my-net.bin.gz:ro" \
  -v "$PWD/analysis_config.cfg:/models/analysis_config.cfg:ro" \
  -e KATAGO_MODEL_PATH=/models/my-net.bin.gz \
  -e KATAGO_CONFIG_PATH=/models/analysis_config.cfg \
  ghcr.io/goban-app/katago-server:latest

# bring your own KataGo binary
docker run -p 2718:2718 -v /path/to/models:/models:ro ghcr.io/goban-app/katago-server:latest-minimal
```

Build locally with `docker build --target <variant> -t katago-server:<variant> .`.
Build arguments: `KATAGO_VERSION` (git tag, default `v1.18.2`), `STANDARD_MODEL`
and `HUMAN_MODEL` (network file names), `STANDARD_MODEL_SHA256` and
`HUMAN_MODEL_SHA256` (verify the downloads when set), `CUDA_VERSION`, `RUST_VERSION`.

## Docker Compose

`docker-compose.yml` in the repository starts the CPU image with a health check
and contains commented GPU and minimal services. Run `docker compose up -d`.

## Helm

```bash
helm repo add katago-server https://goban-app.github.io/katago-server
helm repo update
helm install katago katago-server/katago-server --version 1.8.0
```

Useful values (see `charts/katago-server/values.yaml` for all):

| Value | Purpose |
|---|---|
| `image.variant` | `""` (cpu), `-gpu`, `-combo-cpu`, `-minimal`, ... |
| `resources` | CPU images want 2 to 4 cores and 1 to 2 GiB; GPU images 2 to 4 GiB plus one GPU. |
| `gpu.enabled`, `gpu.count`, `gpu.vendor` | Requests `nvidia.com/gpu` (or `amd.com/gpu`). |
| `config.logLevel`, `config.logFormat` | `RUST_LOG` and `KATAGO_SERVER_LOG_FORMAT`. |
| `config.katago.moveTimeoutSecs`, `config.katago.analysisConfig` | Engine timeout and the mounted `analysis_config.cfg`. |
| `config.customConfig` | A full `config.toml`, mounted and selected via `KATAGO_CONFIG_FILE`. |
| `config.customModel.*` | Init container that downloads a network (optionally checksum-verified) before start. |
| `autoscaling`, `podDisruptionBudget`, `ingress`, `serviceMonitor` | Standard extras. `serviceMonitor` scrapes `/metrics`. |

Probes: liveness on `/api/v1/health/live`, readiness and startup on
`/api/v1/health/ready`. The startup probe allows several minutes because loading
a large network on CPU is slow.

## systemd

`katago-server.service` runs `/usr/local/bin/katago-server` with
`KATAGO_CONFIG_FILE=/opt/katago-server/config.toml`, restarts on failure, and
stops with SIGTERM. `make install` copies the binary and unit; then run
`systemctl enable --now katago-server`.

## Reverse proxy

The server has no authentication or TLS. Put it behind a reverse proxy or a
network policy, restrict `server.cors_allowed_origins`, and give the proxy a
read timeout at least as long as `server.request_timeout_secs` (300 s by
default) so long game analyses are not cut off.

## Shutdown

On SIGTERM or SIGINT the HTTP listener stops accepting connections and in-flight
requests finish (bounded by the request timeout). The server then sends KataGo
`terminate_all`, closes its stdin, waits up to 5 s, and kills it if needed. New
requests during shutdown get 503 `shutting-down`. Set Kubernetes
`terminationGracePeriodSeconds` above your request timeout if every request
must finish.

## Observability

Prometheus metrics at `/metrics`:

| Metric | Type | Labels |
|---|---|---|
| `http_requests_total` | counter | `method`, `route`, `status` |
| `http_request_duration_seconds` | histogram | `method`, `route` |
| `http_requests_in_flight` | gauge | |
| `katago_analysis_requests_total` | counter | `outcome` = `ok`, `timeout`, `rejected`, `unavailable`, `error` |
| `katago_analysis_duration_seconds` | histogram | `outcome` |
| `katago_engine_up` | gauge | 1 while KataGo runs |
| `katago_engine_restarts_total` | counter | |

Logs are text by default or JSON with `KATAGO_SERVER_LOG_FORMAT=json`. Each
request is traced under an `http` span carrying `method`, `path` and
`request_id`; the `x-request-id` response header holds the same id (yours if you
sent one).

## Sizing

- One KataGo process per server instance. Scale horizontally with more replicas rather than huge thread counts.
- CPU: the b28 network takes seconds per position at 50 visits on a few cores. Prefer the smaller b18 network for latency-sensitive CPU deployments.
- GPU: a single mid-range NVIDIA GPU handles hundreds of visits per second; raise `numAnalysisThreads`, `nnMaxBatchSize` and `max_concurrent_requests` accordingly.
- Set `katago.max_visits_limit` on public deployments so a single request cannot monopolise the engine.
