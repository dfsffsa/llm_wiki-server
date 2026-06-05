#!/usr/bin/env bash
# Local E2E: headless server + CivilCareer wiki (no Docker required).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${LLM_WIKI_PROJECT:-${ROOT}/../llm_wiki_projects/CivilCareer}"
TOKEN="${LLM_WIKI_API_TOKEN:-e2e-test-token}"
PORT="${LLM_WIKI_PORT:-8080}"
BASE="http://127.0.0.1:${PORT}"
SERVER="${ROOT}/overlay/server/target/release/llm-wiki-server"
STATIC="${ROOT}/upstream/dist"
CONFIG="${ROOT}/overlay/config/server.example.json"
PID_FILE="/tmp/llm-wiki-e2e.pid"

if [[ ! -x "${SERVER}" ]]; then
  echo "error: server binary missing — run: cargo build --release --manifest-path overlay/server/Cargo.toml" >&2
  exit 1
fi
if [[ ! -d "${STATIC}" ]]; then
  echo "error: UI dist missing — run: VITE_BACKEND=http ./scripts/build-web.sh" >&2
  exit 1
fi
if [[ ! -d "${PROJECT}/wiki" ]]; then
  echo "error: wiki project not found: ${PROJECT}" >&2
  exit 1
fi

cleanup() {
  if [[ -f "${PID_FILE}" ]]; then
    kill "$(cat "${PID_FILE}")" 2>/dev/null || true
    rm -f "${PID_FILE}"
  fi
}
trap cleanup EXIT

# Stop anything already on the port
fuser -k "${PORT}/tcp" 2>/dev/null || true
sleep 1

LLM_WIKI_PROJECT="${PROJECT}" \
LLM_WIKI_API_TOKEN="${TOKEN}" \
LLM_WIKI_BIND="127.0.0.1:${PORT}" \
LLM_WIKI_STATIC="${STATIC}" \
LLM_WIKI_CONFIG="${CONFIG}" \
  "${SERVER}" >/tmp/llm-wiki-e2e.log 2>&1 &
echo $! > "${PID_FILE}"

for i in $(seq 1 30); do
  if curl -fsS "${BASE}/api/v1/health?token=${TOKEN}" >/dev/null 2>&1; then
    break
  fi
  sleep 1
  if [[ "${i}" -eq 30 ]]; then
    echo "error: server failed to start" >&2
    tail -30 /tmp/llm-wiki-e2e.log >&2
    exit 1
  fi
done

echo "==> Health"
curl -fsS "${BASE}/api/v1/health?token=${TOKEN}" | python3 -m json.tool | head -15

PROJECTS_JSON="$(curl -fsS "${BASE}/api/v1/projects?token=${TOKEN}")"
PROJECT_ID="$(echo "${PROJECTS_JSON}" | python3 -c "import json,sys; d=json.load(sys.stdin); p=d.get('currentProject') or (d.get('projects') or [None])[0]; print(p['id'] if p else '')")"
echo "project: ${PROJECT_ID}"

echo "==> wiki/index.md (first 300 chars)"
curl -fsS "${BASE}/api/v1/projects/${PROJECT_ID}/files/content?token=${TOKEN}&path=wiki/index.md" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('content','')[:300])"

echo "==> Search: 职场"
curl -fsS -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/search?token=${TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{"query":"职场","topK":3}' \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print('hits', d.get('tokenHits'), 'mode', d.get('mode')); [print(f\"  - {r['title']}: {r['path']}\") for r in d.get('results',[])[:3]]"

echo "==> Static UI"
curl -fsS "${BASE}/" | grep -q '<div id="root">' && echo "index.html ok"

echo ""
echo "Local E2E passed."
echo "Browse: ${BASE}/"
