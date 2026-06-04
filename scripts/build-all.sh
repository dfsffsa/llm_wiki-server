#!/usr/bin/env bash
# Build upstream web UI (and overlay server when implemented).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UPSTREAM="${ROOT}/upstream"

if [[ ! -f "${UPSTREAM}/package.json" ]]; then
  echo "error: upstream not found. Run: git submodule update --init --recursive"
  exit 1
fi

echo "==> Building upstream frontend..."
npm install --prefix "${UPSTREAM}"
npm run build --prefix "${UPSTREAM}"

if [[ -f "${ROOT}/overlay/server/Cargo.toml" ]]; then
  echo "==> Building overlay server..."
  cargo build --release --manifest-path "${ROOT}/overlay/server/Cargo.toml"
fi

echo "done. UI output: ${UPSTREAM}/dist"
