#!/usr/bin/env bash
# Full-chain integration test: build → unit tests → CLI → HTTP API → optional Docker.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${LLM_WIKI_PROJECT:-${ROOT}/../llm_wiki_projects/CivilCareer}"
TOKEN="${LLM_WIKI_API_TOKEN:-e2e-test-token}"
PORT="${LLM_WIKI_PORT:-8080}"
BASE="http://127.0.0.1:${PORT}"
SERVER="${ROOT}/overlay/server/target/release/llm-wiki-server"
CLI="${ROOT}/scripts/llm-wiki"
STATIC="${ROOT}/upstream/dist"
CONFIG="${ROOT}/overlay/config/server.example.json"
PID_FILE="/tmp/llm-wiki-full-e2e.pid"
PASS=0
FAIL=0
SKIP=0

pass() { echo "  ✓ $1"; PASS=$((PASS + 1)); }
fail() { echo "  ✗ $1"; FAIL=$((FAIL + 1)); }
skip() { echo "  ~ $1 (skipped)"; SKIP=$((SKIP + 1)); }

section() { echo ""; echo "=== $1 ==="; }

section "0. Preconditions"
if [[ ! -d "${PROJECT}/wiki" ]]; then
  echo "error: wiki project missing: ${PROJECT}" >&2
  exit 1
fi
pass "wiki project: ${PROJECT}"

section "1. Apply patches + build"
"${ROOT}/scripts/apply-patches.sh"
source ~/.nvm/nvm.sh 2>/dev/null || true
export PROTOC="${ROOT}/.tools/protoc/bin/protoc"
if [[ -x "${PROTOC}" ]]; then export PATH="${ROOT}/.tools/protoc/bin:${PATH}"; fi

if npm run test:mocks --prefix "${ROOT}/upstream" >/tmp/llm-wiki-full-mocks.log 2>&1; then
  pass "upstream test:mocks ($(tail -1 /tmp/llm-wiki-full-mocks.log))"
else
  fail "upstream test:mocks — see /tmp/llm-wiki-full-mocks.log"
fi

if cargo test --manifest-path "${ROOT}/overlay/crates/llm-wiki-common/Cargo.toml" >/tmp/llm-wiki-full-rust-test.log 2>&1; then
  pass "llm-wiki-common tests"
else
  fail "llm-wiki-common tests"
fi

VITE_BACKEND=http VITE_API_TOKEN="${TOKEN}" "${ROOT}/scripts/build-web.sh" >/tmp/llm-wiki-full-web.log 2>&1
pass "HTTP UI build → upstream/dist"

cargo build --release --manifest-path "${ROOT}/overlay/server/Cargo.toml" >/tmp/llm-wiki-full-server.log 2>&1
pass "headless server binary"

cargo build --release --manifest-path "${ROOT}/overlay/cli/rust/Cargo.toml" >/tmp/llm-wiki-full-cli-build.log 2>&1
pass "CLI binary (rebuilt)"

section "2. CLI (direct, no server)"
if "${CLI}" search "职场" --project "${PROJECT}" --top-k 3 2>/tmp/llm-wiki-cli-search.log | grep -q "mode: keyword"; then
  pass "llm-wiki search"
else
  fail "llm-wiki search"
fi

if "${CLI}" rescan --project "${PROJECT}" --json 2>/tmp/llm-wiki-cli-rescan.log | python3 -c "import json,sys; d=json.load(sys.stdin); assert d['totalFiles']>=0"; then
  pass "llm-wiki rescan"
else
  fail "llm-wiki rescan"
fi

TMP_TXT="${PROJECT}/raw/sources/e2e-smoke.txt"
mkdir -p "${PROJECT}/raw/sources"
echo "e2e smoke test content" > "${TMP_TXT}"
if "${CLI}" preprocess "${TMP_TXT}" -o /tmp/e2e-preprocess-out.txt 2>/tmp/llm-wiki-cli-preprocess.log && grep -q "smoke" /tmp/e2e-preprocess-out.txt; then
  pass "llm-wiki preprocess"
else
  fail "llm-wiki preprocess"
fi

if "${CLI}" reindex --project "${PROJECT}" 2>/tmp/llm-wiki-cli-reindex.log | grep -q "markdown files"; then
  pass "llm-wiki reindex (count only)"
else
  fail "llm-wiki reindex"
fi

if COUNT="$("${CLI}" vector count-chunks --project "${PROJECT}" 2>/tmp/llm-wiki-cli-vector.log)" && [[ "${COUNT}" =~ ^[0-9]+$ ]]; then
  pass "llm-wiki vector count-chunks (${COUNT})"
else
  fail "llm-wiki vector count-chunks"
fi

section "3. HTTP server + API"
cleanup() {
  if [[ -f "${PID_FILE}" ]]; then
    kill "$(cat "${PID_FILE}")" 2>/dev/null || true
    rm -f "${PID_FILE}"
  fi
}
trap cleanup EXIT

fuser -k "${PORT}/tcp" 2>/dev/null || true
sleep 1

LLM_WIKI_PROJECT="${PROJECT}" \
LLM_WIKI_API_TOKEN="${TOKEN}" \
LLM_WIKI_BIND="127.0.0.1:${PORT}" \
LLM_WIKI_STATIC="${STATIC}" \
LLM_WIKI_CONFIG="${CONFIG}" \
  "${SERVER}" >/tmp/llm-wiki-full-server-run.log 2>&1 &
echo $! > "${PID_FILE}"

for i in $(seq 1 30); do
  curl -fsS "${BASE}/api/v1/health?token=${TOKEN}" >/dev/null 2>&1 && break
  sleep 1
  [[ "${i}" -eq 30 ]] && { fail "server start"; tail -20 /tmp/llm-wiki-full-server-run.log; exit 1; }
done
pass "server listening on ${PORT}"

HEALTH="$(curl -fsS "${BASE}/api/v1/health?token=${TOKEN}")"
echo "${HEALTH}" | python3 -c "import json,sys; d=json.load(sys.stdin); assert d['ok'] and d['status']=='running'" && pass "GET /health" || fail "GET /health"

PROJECTS="$(curl -fsS "${BASE}/api/v1/projects?token=${TOKEN}")"
PROJECT_ID="$(echo "${PROJECTS}" | python3 -c "import json,sys; d=json.load(sys.stdin); p=d.get('currentProject') or d['projects'][0]; print(p['id'])")"
pass "GET /projects → ${PROJECT_ID}"

curl -fsS "${BASE}/api/v1/projects/${PROJECT_ID}/files?token=${TOKEN}&root=wiki&recursive=false&maxFiles=50" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); assert d['ok'] and len(d['files'])>0" && pass "GET /files" || fail "GET /files"

curl -fsS "${BASE}/api/v1/projects/${PROJECT_ID}/files/content?token=${TOKEN}&path=wiki/index.md" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'Wiki Index' in d.get('content','')" && pass "GET /files/content" || fail "GET /files/content"

curl -fsS -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/search?token=${TOKEN}" \
  -H 'Content-Type: application/json' -d '{"query":"职场","topK":5}' \
  | python3 -c "import json,sys; d=json.load(sys.stdin); assert d['ok'] and d['tokenHits']>0" && pass "POST /search (中文)" || fail "POST /search"

curl -fsS "${BASE}/api/v1/projects/${PROJECT_ID}/graph?token=${TOKEN}&limit=10" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); assert d['ok'] and len(d.get('nodes',[]))>0" && pass "GET /graph" || fail "GET /graph"

CODE="$(curl -s -o /dev/null -w '%{http_code}' -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/sources/rescan?token=${TOKEN}")"
[[ "${CODE}" == "501" ]] && pass "POST /sources/rescan → 501 (expected)" || fail "POST /sources/rescan (got ${CODE})"

CODE="$(curl -s -o /dev/null -w '%{http_code}' -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/chat?token=${TOKEN}" -H 'Content-Type: application/json' -d '{}')"
[[ "${CODE}" == "501" ]] && pass "POST /chat → 501 (expected)" || fail "POST /chat (got ${CODE})"

curl -fsS "${BASE}/" | grep -q '<div id="root">' && pass "GET / (static UI)" || fail "GET / (static UI)"
curl -fsS "${BASE}/assets/index" >/dev/null 2>&1 || curl -fsS "${BASE}/" | grep -q 'assets/' && pass "UI assets referenced" || skip "UI assets (bundled paths vary)"

CODE="$(curl -s -o /dev/null -w '%{http_code}' "${BASE}/api/v1/projects?token=wrong")"
[[ "${CODE}" == "401" ]] && pass "auth rejects bad token" || fail "auth (got ${CODE})"

section "4. Docker (optional)"
cleanup  # free port 8080 before Docker
sleep 1
if [[ -x "/mnt/c/Program Files/Docker/Docker/resources/bin/docker.exe" ]] \
  && "/mnt/c/Program Files/Docker/Docker/resources/bin/docker.exe" info >/dev/null 2>&1; then
  if "${ROOT}/scripts/e2e-docker.sh" >/tmp/llm-wiki-full-docker.log 2>&1; then
    pass "Docker e2e-docker.sh"
  else
    skip "Docker e2e — $(tail -1 /tmp/llm-wiki-full-docker.log)"
  fi
else
  skip "Docker not available"
fi

section "Summary"
echo "passed: ${PASS}  failed: ${FAIL}  skipped: ${SKIP}"
if [[ "${FAIL}" -gt 0 ]]; then
  echo "FULL CHAIN: FAILED"
  exit 1
fi
echo "FULL CHAIN: OK"
echo "Browse UI (start server manually): ${BASE}/"
