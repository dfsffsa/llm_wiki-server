#!/usr/bin/env bash
# Build Rust CLI + install Node CLI dependencies.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROTOC_DIR="${ROOT}/.tools/protoc"
PROTOC_BIN="${PROTOC_DIR}/bin/protoc"

ensure_protoc() {
  if command -v protoc >/dev/null 2>&1; then
    return 0
  fi
  if [[ -x "${PROTOC_BIN}" ]]; then
    export PROTOC="${PROTOC_BIN}"
    export PATH="${PROTOC_DIR}/bin:${PATH}"
    return 0
  fi

  echo "==> Installing protoc (required by lancedb)..."
  mkdir -p "${ROOT}/.tools"
  local version="28.3"
  local zip="${ROOT}/.tools/protoc-${version}.zip"
  curl -fsSL -o "${zip}" \
    "https://github.com/protocolbuffers/protobuf/releases/download/v${version}/protoc-${version}-linux-x86_64.zip"
  rm -rf "${PROTOC_DIR}"
  mkdir -p "${PROTOC_DIR}"
  python3 -c "import zipfile; zipfile.ZipFile('${zip}').extractall('${PROTOC_DIR}')"
  chmod +x "${PROTOC_BIN}"
  export PROTOC="${PROTOC_BIN}"
  export PATH="${PROTOC_DIR}/bin:${PATH}"
}

ensure_protoc

echo "==> Building Rust CLI (llm-wiki)..."
cargo build --release --manifest-path "${ROOT}/overlay/cli/rust/Cargo.toml"

if [[ -f "${ROOT}/overlay/cli/node/package.json" ]]; then
  echo "==> Installing Node CLI dependencies..."
  if command -v node >/dev/null 2>&1; then
    npm install --prefix "${ROOT}/overlay/cli/node"
  else
    echo "warning: node not found — skip npm install (ingest/reindex --vectors need Node)"
  fi
fi

echo "done."
echo "  binary: ${ROOT}/overlay/cli/rust/target/release/llm-wiki"
echo "  wrapper: ${ROOT}/scripts/llm-wiki"
