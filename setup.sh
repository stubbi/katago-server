#!/usr/bin/env bash
# Local development setup: find or download KataGo and a network, then write
# analysis_config.cfg and config.toml next to this script.
set -euo pipefail

cd "$(dirname "$0")"

KATAGO_VERSION="v1.18.2"
MODEL_NAME="kata1-b28c512nbt-s12043015936-d5616446734.bin.gz"
MODEL_URL="https://media.katagotraining.org/uploaded/networks/models/kata1/${MODEL_NAME}"

log() { printf '\n==> %s\n' "$*"; }

download() {
  local url="$1" out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl --fail --location --retry 3 --progress-bar -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "$out" "$url"
  else
    echo "error: need curl or wget" >&2
    exit 1
  fi
}

os="$(uname -s)"
arch="$(uname -m)"
katago_path=""
model_path=""

log "Locating KataGo (${os}/${arch})"
if command -v katago >/dev/null 2>&1; then
  katago_path="$(command -v katago)"
  echo "using katago on PATH: ${katago_path} ($("$katago_path" version 2>/dev/null | head -1 || true))"
  if command -v brew >/dev/null 2>&1; then
    brew_share="$(brew --prefix)/share/katago"
    if [ -d "$brew_share" ]; then
      # Prefer the newest kata1 network Homebrew ships.
      model_path="$(find "$brew_share" -maxdepth 1 -name 'kata1-*.bin.gz' | sort | tail -1 || true)"
      [ -n "$model_path" ] && echo "using bundled network: ${model_path}"
    fi
  fi
elif [ "$os" = "Darwin" ]; then
  cat >&2 <<MSG
KataGo does not publish macOS binaries for this platform. Install it with Homebrew:

    brew install katago

then run ./setup.sh again.
MSG
  exit 1
elif [ "$os" = "Linux" ] && [ "$arch" = "x86_64" ]; then
  if [ ! -x ./katago ]; then
    zip="katago-${KATAGO_VERSION}-eigen-linux-x64.zip"
    log "Downloading KataGo ${KATAGO_VERSION} (Eigen/CPU build)"
    download "https://github.com/lightvector/KataGo/releases/download/${KATAGO_VERSION}/${zip}" "$zip"
    unzip -q -o "$zip" katago
    chmod +x katago
    rm -f "$zip"
  fi
  katago_path="$(pwd)/katago"
  echo "katago: ${katago_path}"
else
  cat >&2 <<MSG
No prebuilt KataGo for ${os}/${arch}. Build it from source
(https://github.com/lightvector/KataGo/blob/master/Compiling.md), put the
binary on your PATH and run ./setup.sh again.
MSG
  exit 1
fi

if [ -z "$model_path" ]; then
  log "Fetching network ${MODEL_NAME}"
  if [ ! -f "$MODEL_NAME" ]; then
    download "$MODEL_URL" "$MODEL_NAME"
  fi
  model_path="$(pwd)/${MODEL_NAME}"
fi
echo "network: ${model_path}"

log "Writing configuration"
if [ ! -f analysis_config.cfg ]; then
  cp analysis_config.cfg.example analysis_config.cfg
  echo "created analysis_config.cfg"
else
  echo "analysis_config.cfg exists, keeping it"
fi

if [ ! -f config.toml ]; then
  cat > config.toml <<TOML
[server]
host = "::"
port = 2718

[katago]
katago_path = "${katago_path}"
model_path = "${model_path}"
config_path = "$(pwd)/analysis_config.cfg"
move_timeout_secs = 60
default_max_visits = 50
TOML
  echo "created config.toml"
else
  echo "config.toml exists, keeping it"
fi

log "Building"
cargo build --release

cat <<MSG

Setup complete.

  start:      ./target/release/katago-server
  check:      ./target/release/katago-server check-config
  smoke test: ./test.sh          (in another terminal, once the server runs)
  docs:       http://localhost:2718/docs
MSG
