#!/usr/bin/env bash
# Build upstream UI for HTTP (headless server) or desktop (default upstream vite).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UPSTREAM="${ROOT}/upstream"
MODE="${VITE_BACKEND:-http}"

# WSL fallback: Windows Node.js when Linux node is missing
if ! command -v node >/dev/null 2>&1; then
  for candidate in /mnt/d/software/nodejs/node.exe /mnt/c/Program\ Files/nodejs/node.exe; do
    if [[ -x "${candidate}" ]]; then
      export PATH="$(dirname "${candidate}"):${PATH}"
      break
    fi
  done
fi
if command -v node.exe >/dev/null 2>&1 && ! command -v node >/dev/null 2>&1; then
  TOOLS="${ROOT}/.tools"
  mkdir -p "${TOOLS}"
  ln -sf "$(command -v node.exe)" "${TOOLS}/node"
  export PATH="${TOOLS}:${PATH}"
fi

if [[ ! -f "${UPSTREAM}/package.json" ]]; then
  echo "error: upstream not found. Run: git submodule update --init --recursive"
  exit 1
fi

echo "==> Installing upstream frontend dependencies..."
npm install --prefix "${UPSTREAM}"

if [[ "${MODE}" == "http" ]]; then
  echo "==> Building HTTP read-only UI (VITE_BACKEND=http)..."
  (
    cd "${UPSTREAM}"
    VITE_BACKEND=http \
    VITE_API_TOKEN="${VITE_API_TOKEN:-}" \
    VITE_API_BASE="${VITE_API_BASE:-}" \
    npm run build
  )
else
  echo "==> Building desktop UI (upstream/vite.config.ts)..."
  npm run build --prefix "${UPSTREAM}"
fi

LITE_SRC="${ROOT}/overlay/static/lite"
LITE_DST="${UPSTREAM}/dist/lite"
if [[ -d "${LITE_SRC}" ]]; then
  echo "==> Copying Lite static UI to dist/lite..."
  mkdir -p "${LITE_SRC}/vendor"
  if [[ -f "${UPSTREAM}/node_modules/marked/lib/marked.esm.js" ]]; then
    cp "${UPSTREAM}/node_modules/marked/lib/marked.esm.js" "${LITE_SRC}/vendor/"
    cp "${UPSTREAM}/node_modules/dompurify/dist/purify.es.mjs" "${LITE_SRC}/vendor/purify.es.js"
  fi
  mkdir -p "${LITE_DST}"
  cp -r "${LITE_SRC}/." "${LITE_DST}/"
  TOKEN="${VITE_API_TOKEN:-e2e-test-token}"
  cat > "${LITE_DST}/config.js" <<EOF
window.LLM_WIKI_LITE_CONFIG = {
  apiBase: "${VITE_API_BASE:-}",
  apiToken: "${TOKEN}",
};
EOF
fi

echo "done. UI output: ${UPSTREAM}/dist"
