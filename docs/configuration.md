# Configuration

Settings are resolved in this order, later sources winning:

1. built-in defaults
2. `config.toml`: the file given by `--config FILE` or `KATAGO_CONFIG_FILE`, else `./config.toml` if it exists
3. environment variables

A present but unparseable environment variable is an error, not ignored. Unknown
keys in `config.toml` are rejected. `katago-server check-config` loads, validates
(including that the binary, network and KataGo config exist) and prints the
effective configuration as TOML.

## `[server]`

| Key | Type | Default | Env var | Description |
|---|---|---|---|---|
| `host` | string | `"::"` | `KATAGO_SERVER_HOST` | Bind address. `::` serves IPv6 and IPv4 on most systems. |
| `port` | integer | `2718` | `KATAGO_SERVER_PORT` | TCP port. |
| `request_timeout_secs` | integer | `300` | `KATAGO_SERVER_REQUEST_TIMEOUT_SECS` | Upper bound on any single HTTP request. Must be at least `katago.move_timeout_secs`. |
| `max_concurrent_requests` | integer | `256` | `KATAGO_SERVER_MAX_CONCURRENT_REQUESTS` | In-flight requests before load shedding with 503. |
| `max_body_bytes` | integer | `1048576` | `KATAGO_SERVER_MAX_BODY_BYTES` | Maximum request body. At least 1024. |
| `cors_allowed_origins` | list of strings | `["*"]` | `KATAGO_SERVER_CORS_ALLOWED_ORIGINS` (comma-separated) | `*` allows any origin; otherwise an explicit list. Must not be empty. |
| `log_format` | `text` or `json` | `text` | `KATAGO_SERVER_LOG_FORMAT` | JSON emits one object per line for log aggregators. |

## `[katago]`

| Key | Type | Default | Env var | Description |
|---|---|---|---|---|
| `katago_path` | path | `./katago` | `KATAGO_KATAGO_PATH` | The `katago` executable. |
| `model_path` | path | `./model.bin.gz` | `KATAGO_MODEL_PATH` | Main neural network. |
| `human_model_path` | path | unset | `KATAGO_HUMAN_MODEL_PATH` | Human SL network; enables `humanSLProfile` overrides. Empty string unsets. |
| `config_path` | path | `./analysis_config.cfg` | `KATAGO_CONFIG_PATH` | KataGo analysis engine `.cfg`. |
| `move_timeout_secs` | integer | `20` | `KATAGO_MOVE_TIMEOUT_SECS` | Silence tolerated per analysed turn before the query is terminated and 504 returned. |
| `default_max_visits` | integer | `10` | `KATAGO_DEFAULT_MAX_VISITS` | `maxVisits` when the request omits it. `0` or empty unsets it, leaving the `.cfg` value in charge. |
| `max_visits_limit` | integer | unset | `KATAGO_MAX_VISITS_LIMIT` | Requests (including `overrideSettings.maxVisits`) above this get 400. |
| `max_restart_attempts` | integer | `10` | `KATAGO_MAX_RESTART_ATTEMPTS` | Restarts after KataGo exits before the server gives up and liveness fails. |

Validation rules: `port` > 0, timeouts > 0, `request_timeout_secs >= move_timeout_secs`,
`default_max_visits <= max_visits_limit` when both are set.

## Logging

`RUST_LOG` filters output (default `info,katago_server=info,tower_http=info`).
`RUST_LOG=debug` also shows every line exchanged with KataGo under the
`katago::stdin`, `katago::stdout` and `katago::stderr` targets. When KataGo exits
unexpectedly the last 40 stderr lines are logged at error level.

## Example `config.toml`

```toml
[server]
host = "::"
port = 2718
cors_allowed_origins = ["https://goban.app"]
log_format = "json"

[katago]
katago_path = "/usr/local/bin/katago"
model_path = "/models/kata1-b28c512nbt-s12043015936-d5616446734.bin.gz"
config_path = "/etc/katago/analysis_config.cfg"
move_timeout_secs = 30
default_max_visits = 50
max_visits_limit = 2000
```

## Tuning the KataGo analysis config

The server starts `katago analysis -model ... -config <config_path>`. The shipped
`analysis_config.cfg.*` files are starting points:

| Setting | CPU | GPU | Meaning |
|---|---|---|---|
| `numAnalysisThreads` | 1 to 2 | 4 to 8 | Positions searched in parallel. Game analysis benefits directly. |
| `numSearchThreadsPerAnalysisThread` | 1 to 2 | 4 to 8 | Threads per position. |
| `nnMaxBatchSize` | 8 | 32 to 64 | Neural network batch size. |
| `numNNServerThreadsPerModel` | 1 to 2 | 1 to 2 | One per GPU is typical. |
| `maxVisits` | 10 to 100 | 200 to 2000 | Default visits when neither the request nor `default_max_visits` sets one. |
| `nnCacheSizePowerOfTwo` | 18 to 20 | 20 to 23 | Cache entries as a power of two; memory grows accordingly. |

Keep `reportAnalysisWinratesAs = SIDETOMOVE` so the API semantics hold. The
shipped configs set `logToStderr = true` and no `logDir`, so KataGo writes no log
files; the server captures stderr instead. Keep `move_timeout_secs` above the
time a single position takes at your `maxVisits`.
