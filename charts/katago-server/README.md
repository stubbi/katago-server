# katago-server Helm chart

Deploys [katago-server](https://github.com/goban-app/katago-server), a REST API in front of the KataGo Go engine.

## Install

```bash
helm repo add katago-server https://goban-app.github.io/katago-server
helm repo update
helm install katago katago-server/katago-server --namespace katago --create-namespace
```

The pod becomes Ready once KataGo has loaded its network (a few minutes on CPU). Then:

```bash
kubectl -n katago port-forward svc/katago-katago-server 2718:2718
curl http://localhost:2718/api/v1/health
curl -X POST http://localhost:2718/api/v1/analysis \
  -H 'Content-Type: application/json' -d '{"moves":["D4","Q16"],"maxVisits":50}'
```

Interactive API docs are served at `/docs`. See [values-examples.yaml](values-examples.yaml) for CPU, GPU, minimal, custom-network and production setups.

## Image variants

Set `image.variant` to pick a build of `ghcr.io/goban-app/katago-server`:

| Variant | Contents |
|---|---|
| `""` or `-cpu` | KataGo (Eigen) + standard 28-block network |
| `-gpu` | KataGo (CUDA) + standard network; set `gpu.enabled: true` |
| `-human-cpu`, `-human-gpu` | Human SL network only |
| `-combo-cpu`, `-combo-gpu` | Standard + Human SL network |
| `-minimal` | Server only; mount KataGo, network and config and set `config.katago.*` paths |
| `-base` | Server binary only |

## Configuration

Each `config` value maps to one environment variable of the server. Unset values keep the image defaults.

| Value | Env var | Default |
|---|---|---|
| `config.logLevel` | `RUST_LOG` | `info` |
| `config.logFormat` | `KATAGO_SERVER_LOG_FORMAT` (`text`/`json`) | `""` |
| `config.corsAllowedOrigins` | `KATAGO_SERVER_CORS_ALLOWED_ORIGINS` | `[]` (any) |
| `config.maxConcurrentRequests` | `KATAGO_SERVER_MAX_CONCURRENT_REQUESTS` | unset |
| `config.requestTimeoutSecs` | `KATAGO_SERVER_REQUEST_TIMEOUT_SECS` | unset |
| `config.katago.path` | `KATAGO_KATAGO_PATH` | `""` |
| `config.katago.modelPath` | `KATAGO_MODEL_PATH` | `""` |
| `config.katago.humanModelPath` | `KATAGO_HUMAN_MODEL_PATH` | `""` |
| `config.katago.configPath` | `KATAGO_CONFIG_PATH` | `""` |
| `config.katago.moveTimeoutSecs` | `KATAGO_MOVE_TIMEOUT_SECS` | `60` |
| `config.katago.defaultMaxVisits` | `KATAGO_DEFAULT_MAX_VISITS` | unset |
| `config.katago.maxVisitsLimit` | `KATAGO_MAX_VISITS_LIMIT` | unset |
| `config.katago.maxRestartAttempts` | `KATAGO_MAX_RESTART_ATTEMPTS` | unset |
| `config.katago.analysisConfig` | mounted at `/app/analysis_config.cfg` | CPU 1x1 threads |
| `config.customConfig` | mounted at `/config/config.toml`, selected via `KATAGO_CONFIG_FILE` | `""` |
| `config.customModel.*` | init container downloads a network to `/app/models/<filename>` and sets `KATAGO_MODEL_PATH` | disabled |

Environment variables always override `customConfig`. Use `env` for anything not listed (for example `KATAGO_SERVER_MAX_BODY_BYTES`).

Standard Kubernetes knobs are available as usual: `replicaCount`, `resources`, `autoscaling`, `podDisruptionBudget`, `ingress`, `service`, `serviceAccount`, `nodeSelector`, `tolerations`, `affinity`, `topologySpreadConstraints`, `volumes`, `volumeMounts`, `env`, `envFrom`, `priorityClassName`, `lifecycle`. `gpu.enabled` adds `nvidia.com/gpu` (or `amd.com/gpu`) to the limits.

## Probes

| Probe | Path | Meaning |
|---|---|---|
| startup | `/api/v1/health/ready` | up to 60 x 5s for the network to load |
| readiness | `/api/v1/health/ready` | KataGo has answered a query |
| liveness | `/api/v1/health/live` | fails only when KataGo is down and the server has exhausted its restart budget |

## Monitoring

`serviceMonitor.enabled: true` creates a Prometheus Operator `ServiceMonitor` scraping `/metrics` (request counts and latencies per route, KataGo query durations, engine up/restarts).

## Development

```bash
helm lint charts/katago-server
helm template katago charts/katago-server
helm template katago charts/katago-server --set config.customModel.enabled=true --set config.customModel.url=https://example/net.bin.gz
```
