#!/usr/bin/env bash
# Build upstream web UI (and overlay server when implemented).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UPSTREAM="${ROOT}/upstream"

if [[ ! -f "${UPSTREAM}/package.json" ]]; then
  echo "error: upstream not found. Run: git submodule update --init --recursive"
  exit 1
fi

if [[ -x "${ROOT}/scripts/apply-patches.sh" ]]; then
  echo "==> Applying overlay patches to upstream..."
  "${ROOT}/scripts/apply-patches.sh"
fi

echo "==> Building upstream frontend (HTTP read-only)..."
chmod +x "${ROOT}/scripts/build-web.sh"
VITE_BACKEND=http "${ROOT}/scripts/build-web.sh"

if [[ -f "${ROOT}/overlay/server/Cargo.toml" ]]; then
  echo "==> Building overlay server..."
  cargo build --release --manifest-path "${ROOT}/overlay/server/Cargo.toml"
fi

if [[ -f "${ROOT}/overlay/cli/rust/Cargo.toml" ]]; then
  echo "==> Building overlay CLI..."
  chmod +x "${ROOT}/scripts/build-cli.sh"
  "${ROOT}/scripts/build-cli.sh"
fi

echo "done. UI output: ${UPSTREAM}/dist"
echo "      server:    ${ROOT}/overlay/server/target/release/llm-wiki-server"
echo "      cli:       ${ROOT}/scripts/llm-wiki"
