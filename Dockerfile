# syntax=docker/dockerfile:1.7
# ==============================================================================
# katago-server images
#
#   target      contents                                   platforms
#   base        server binary only                         amd64, arm64
#   minimal     server binary, expects /models/* mounted   amd64, arm64
#   cpu         + KataGo (Eigen) + standard network        amd64, arm64
#   human-cpu   + KataGo (Eigen) + Human SL network        amd64, arm64
#   combo-cpu   + KataGo (Eigen) + both networks           amd64, arm64
#   gpu         + KataGo (CUDA)  + standard network        amd64
#   human-gpu   + KataGo (CUDA)  + Human SL network        amd64
#   combo-gpu   + KataGo (CUDA)  + both networks           amd64
#
#   docker build --target cpu -t katago-server:cpu .
# ==============================================================================

ARG RUST_VERSION=1.98
ARG KATAGO_VERSION=v1.18.2
ARG CUDA_VERSION=12.4.1
ARG STANDARD_MODEL=kata1-b28c512nbt-s12043015936-d5616446734.bin.gz
ARG HUMAN_MODEL=b18c384nbt-humanv0.bin.gz
# Optional SHA-256 checksums for the downloaded networks (verified when non-empty)
ARG STANDARD_MODEL_SHA256=""
ARG HUMAN_MODEL_SHA256=""

# ------------------------------------------------------------------------------
# Rust build: a statically linked musl binary, so it runs unchanged on the
# Debian and Ubuntu/CUDA runtime images regardless of their glibc version.
# cargo-chef caches the dependency build.
# ------------------------------------------------------------------------------
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION}-slim AS chef
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add "$(uname -m)-unknown-linux-musl"
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS rust-builder
ARG GIT_SHA=""
ENV GIT_SHA=${GIT_SHA}
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --target "$(uname -m)-unknown-linux-musl" --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin katago-server --target "$(uname -m)-unknown-linux-musl" \
    && cp "target/$(uname -m)-unknown-linux-musl/release/katago-server" /app/katago-server-static

# ------------------------------------------------------------------------------
# KataGo, CPU backend (Eigen). AVX2 only on x86_64.
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS katago-cpu-builder
ARG KATAGO_VERSION
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git build-essential cmake libeigen3-dev libzip-dev zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
RUN git clone --depth 1 --branch "${KATAGO_VERSION}" https://github.com/lightvector/KataGo.git
WORKDIR /build/KataGo/cpp
RUN if [ "$(uname -m)" = "x86_64" ]; then AVX2=1; else AVX2=0; fi \
    && cmake . -DUSE_BACKEND=EIGEN -DUSE_AVX2=${AVX2} -DCMAKE_BUILD_TYPE=Release \
    && make -j"$(nproc)" \
    && strip katago

# ------------------------------------------------------------------------------
# KataGo, CUDA backend
# ------------------------------------------------------------------------------
FROM nvidia/cuda:${CUDA_VERSION}-cudnn-devel-ubuntu22.04 AS katago-gpu-builder
ARG KATAGO_VERSION
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git build-essential cmake libzip-dev zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
RUN git clone --depth 1 --branch "${KATAGO_VERSION}" https://github.com/lightvector/KataGo.git
WORKDIR /build/KataGo/cpp
RUN cmake . -DUSE_BACKEND=CUDA -DCMAKE_BUILD_TYPE=Release \
    && make -j"$(nproc)" \
    && strip katago

# ------------------------------------------------------------------------------
# Runtime base (Debian): non-root user, server binary, health check
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime-base
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libgomp1 libzip4 zlib1g \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 katago \
    && useradd --uid 1000 --gid 1000 --create-home --shell /usr/sbin/nologin katago
WORKDIR /app
COPY --from=rust-builder /app/katago-server-static /app/katago-server
COPY config.toml.example analysis_config.cfg.example /app/
RUN chown -R katago:katago /app
ENV RUST_LOG=info \
    KATAGO_SERVER_HOST=:: \
    KATAGO_SERVER_PORT=2718
EXPOSE 2718
HEALTHCHECK --interval=30s --timeout=10s --start-period=180s --retries=3 \
    CMD ["/app/katago-server", "healthcheck"]
ENTRYPOINT ["/app/katago-server"]

FROM runtime-base AS base
USER 1000:1000

FROM runtime-base AS minimal
ENV KATAGO_KATAGO_PATH=/models/katago \
    KATAGO_MODEL_PATH=/models/model.bin.gz \
    KATAGO_CONFIG_PATH=/models/analysis_config.cfg
VOLUME ["/models"]
USER 1000:1000

# ------------------------------------------------------------------------------
# CPU variants
# ------------------------------------------------------------------------------
FROM runtime-base AS cpu-base
COPY --from=katago-cpu-builder /build/KataGo/cpp/katago /app/katago
COPY docker-setup.sh /app/docker-setup.sh

FROM cpu-base AS cpu
ARG STANDARD_MODEL
ARG STANDARD_MODEL_SHA256
ENV KATAGO_MODEL=${STANDARD_MODEL} \
    KATAGO_MODEL_SHA256=${STANDARD_MODEL_SHA256}
COPY analysis_config.cfg.cpu /app/analysis_config.cfg
RUN ./docker-setup.sh && chown -R katago:katago /app
USER 1000:1000

FROM cpu-base AS human-cpu
ARG HUMAN_MODEL
ARG HUMAN_MODEL_SHA256
ENV KATAGO_MODEL=${HUMAN_MODEL} \
    KATAGO_MODEL_SHA256=${HUMAN_MODEL_SHA256}
COPY analysis_config.cfg.human-cpu /app/analysis_config.cfg
RUN ./docker-setup.sh && chown -R katago:katago /app
USER 1000:1000

FROM cpu-base AS combo-cpu
ARG STANDARD_MODEL
ARG STANDARD_MODEL_SHA256
ARG HUMAN_MODEL
ARG HUMAN_MODEL_SHA256
ENV KATAGO_MODEL=${STANDARD_MODEL} \
    KATAGO_MODEL_SHA256=${STANDARD_MODEL_SHA256} \
    KATAGO_HUMAN_MODEL=${HUMAN_MODEL} \
    KATAGO_HUMAN_MODEL_SHA256=${HUMAN_MODEL_SHA256}
COPY analysis_config.cfg.combo-cpu /app/analysis_config.cfg
RUN ./docker-setup.sh && chown -R katago:katago /app
USER 1000:1000

# ------------------------------------------------------------------------------
# GPU runtime base (CUDA runtime image, ~2GB) and variants
# ------------------------------------------------------------------------------
FROM nvidia/cuda:${CUDA_VERSION}-cudnn-runtime-ubuntu22.04 AS gpu-base
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libgomp1 libzip4 zlib1g \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 katago \
    && useradd --uid 1000 --gid 1000 --create-home --shell /usr/sbin/nologin katago
WORKDIR /app
COPY --from=rust-builder /app/katago-server-static /app/katago-server
COPY --from=katago-gpu-builder /build/KataGo/cpp/katago /app/katago
COPY config.toml.example analysis_config.cfg.example docker-setup.sh /app/
ENV RUST_LOG=info \
    KATAGO_SERVER_HOST=:: \
    KATAGO_SERVER_PORT=2718 \
    NVIDIA_VISIBLE_DEVICES=all \
    NVIDIA_DRIVER_CAPABILITIES=compute,utility
EXPOSE 2718
HEALTHCHECK --interval=30s --timeout=10s --start-period=180s --retries=3 \
    CMD ["/app/katago-server", "healthcheck"]
ENTRYPOINT ["/app/katago-server"]

FROM gpu-base AS gpu
ARG STANDARD_MODEL
ARG STANDARD_MODEL_SHA256
ENV KATAGO_MODEL=${STANDARD_MODEL} \
    KATAGO_MODEL_SHA256=${STANDARD_MODEL_SHA256}
COPY analysis_config.cfg.gpu /app/analysis_config.cfg
RUN ./docker-setup.sh && chown -R katago:katago /app
USER 1000:1000

FROM gpu-base AS human-gpu
ARG HUMAN_MODEL
ARG HUMAN_MODEL_SHA256
ENV KATAGO_MODEL=${HUMAN_MODEL} \
    KATAGO_MODEL_SHA256=${HUMAN_MODEL_SHA256}
COPY analysis_config.cfg.human-gpu /app/analysis_config.cfg
RUN ./docker-setup.sh && chown -R katago:katago /app
USER 1000:1000

FROM gpu-base AS combo-gpu
ARG STANDARD_MODEL
ARG STANDARD_MODEL_SHA256
ARG HUMAN_MODEL
ARG HUMAN_MODEL_SHA256
ENV KATAGO_MODEL=${STANDARD_MODEL} \
    KATAGO_MODEL_SHA256=${STANDARD_MODEL_SHA256} \
    KATAGO_HUMAN_MODEL=${HUMAN_MODEL} \
    KATAGO_HUMAN_MODEL_SHA256=${HUMAN_MODEL_SHA256}
COPY analysis_config.cfg.combo-gpu /app/analysis_config.cfg
RUN ./docker-setup.sh && chown -R katago:katago /app
USER 1000:1000
