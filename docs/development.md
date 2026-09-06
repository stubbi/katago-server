# Development

## Prerequisites

- Rust at least the `rust-version` in `Cargo.toml` (`rustup update stable` is fine)
- `python3` for the fake KataGo used by the end-to-end tests
- Optional: a real KataGo for manual runs. macOS: `brew install katago` (ships the binary and networks under `$(brew --prefix)/share/katago`). Linux: `./setup.sh` downloads KataGo 1.18.2 and a network.
- Optional lint tools: `cargo install cargo-deny`, `shellcheck`, `hadolint`, `helm`

## Layout

```
src/
  main.rs        CLI (serve | healthcheck | check-config), tracing, graceful shutdown
  lib.rs         module tree; the binary is a thin wrapper around the library
  config.rs      Config types, TOML + env loading with injectable env lookup
  coords.rs      GTP coordinate parsing and validation
  error.rs       EngineError
  engine/
    mod.rs       AnalysisEngine: tokio::process supervisor, id-routed responses, restart loop, shutdown
    protocol.rs  Query wire type sent to KataGo
  api/
    mod.rs       Router and middleware stack (request id, tracing, CORS, timeout, load shed, body limit, metrics)
    handlers.rs  HTTP handlers
    types.rs     Request/response types (also used to parse KataGo output)
    validate.rs  AnalysisRequest -> Query; every 400 rule lives here
    problem.rs   RFC 9457 ApiError and the problem-json Json extractor
    openapi.rs   utoipa document
  metrics.rs     Prometheus recorder and HTTP middleware
tests/
  fake_katago.py speaks the KataGo analysis JSON protocol without a network
  common/mod.rs  test harness: boots the router in-process against the fake
  api.rs         end-to-end tests over the HTTP surface
```

## Tests

```bash
make test        # cargo test: unit tests inline in each module + tests/api.rs
```

`tests/api.rs` starts the real engine against `tests/fake_katago.py` through a
generated wrapper script. The fake is steered with keys inside
`overrideSettings` that real KataGo never sees:

| Key | Effect |
|---|---|
| `fakeDelayMs` | sleep before each analysed turn (timeout tests) |
| `fakeCrash` | exit immediately (restart tests) |
| `fakeWarn` | emit a warning line before the result |
| `fakeError` | emit an error without a field (502 path) |

Environment for the fake: `FAKE_KATAGO_STARTUP_DELAY` (seconds),
`FAKE_KATAGO_CRASH_ON_START`, `FAKE_KATAGO_LOG` (file that records every query;
tests assert on it).

## Lint

```bash
make lint        # cargo fmt --check, cargo clippy --all-targets -D warnings (pedantic), cargo deny check
```

Lint levels are set in `Cargo.toml` under `[lints]`. Unsafe code is forbidden.

## Running against a real KataGo

Create `config.toml`:

```toml
[katago]
katago_path = "/opt/homebrew/bin/katago"
model_path = "/opt/homebrew/share/katago/kata1-b18c384nbt-s9996604416-d4316597426.bin.gz"
config_path = "./analysis_config.cfg"
```

Then:

```bash
cp analysis_config.cfg.example analysis_config.cfg
cargo run --release -- check-config
RUST_LOG=debug cargo run --release
make smoke       # test.sh against http://localhost:2718
```

## CI

`.github/workflows/ci.yml` runs on pushes and pull requests:

| Job | What |
|---|---|
| `test` | `cargo test` on stable |
| `lint` | fmt, clippy with `-D warnings` |
| `msrv` | `cargo check` on the `rust-version` toolchain |
| `deny` | `cargo deny check` (advisories, licenses, sources) |
| `scripts` | shellcheck on `*.sh`, hadolint on the Dockerfile, `helm lint` and `helm template` |
| `versions` | Cargo.toml version == Chart.yaml version == appVersion |
| `docker-build` | builds every image variant (pull requests) |
| `docker-smoke` | runs the CPU image, waits for readiness, exercises the API |

Releases are driven by `charts/katago-server/Chart.yaml`; see `RELEASING.md`.
