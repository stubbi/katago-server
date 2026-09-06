#!/usr/bin/env bash
# Build-time helper run inside the Dockerfile from WORKDIR /app.
# Downloads the network(s) named by KATAGO_MODEL / KATAGO_HUMAN_MODEL, verifies
# them when a SHA256 is given, and writes config.toml pointing at them.
set -euo pipefail

KATAGO_MODEL="${KATAGO_MODEL:-}"
KATAGO_MODEL_SHA256="${KATAGO_MODEL_SHA256:-}"
KATAGO_HUMAN_MODEL="${KATAGO_HUMAN_MODEL:-}"
KATAGO_HUMAN_MODEL_SHA256="${KATAGO_HUMAN_MODEL_SHA256:-}"
BASE_URL="https://media.katagotraining.org/uploaded/networks"

model_url() {
  case "$1" in
    *humanv*) echo "${BASE_URL}/models_extra/$1" ;;
    *)        echo "${BASE_URL}/models/kata1/$1" ;;
  esac
}

fetch() { # fetch <name> <expected sha256 or empty>
  local name="$1" sha="$2"
  echo "Downloading ${name}"
  curl --fail --location --retry 3 --retry-delay 5 --silent --show-error \
    -o "$name" "$(model_url "$name")"
  if [ -n "$sha" ]; then
    echo "${sha}  ${name}" | sha256sum -c -
  else
    echo "no checksum provided for ${name}; skipping verification"
  fi
  ls -lh "$name"
}

[ -n "$KATAGO_MODEL" ] && fetch "$KATAGO_MODEL" "$KATAGO_MODEL_SHA256"
[ -n "$KATAGO_HUMAN_MODEL" ] && fetch "$KATAGO_HUMAN_MODEL" "$KATAGO_HUMAN_MODEL_SHA256"

[ -f katago ] && chmod +x katago

if [ -f config.toml.example ]; then
  cp config.toml.example config.toml
  if [ -n "$KATAGO_MODEL" ]; then
    sed -i "s|^model_path = .*|model_path = \"/app/${KATAGO_MODEL}\"|" config.toml
  fi
  if [ -n "$KATAGO_HUMAN_MODEL" ]; then
    sed -i "/^model_path = /a human_model_path = \"/app/${KATAGO_HUMAN_MODEL}\"" config.toml
  fi
  echo "config.toml:"
  cat config.toml
fi

echo "Setup complete"
