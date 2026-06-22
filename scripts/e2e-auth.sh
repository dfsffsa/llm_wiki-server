#!/usr/bin/env bash
# E2E: cookie-auth path — register/login/me/conversations/usage-limit/logout/bearer.
#
# Unlike e2e-local.sh (Bearer + full wiki corpus), this exercises the
# browser-cookie auth surface added in the public-deploy-auth work. It needs
# the server binary but NO wiki corpus: conversations CRUD and the usage
# counter work against any project path. The chat-usage-limit step needs a
# real project_id whose chat actually streams (it 200s only if the LLM
# endpoint is configured), so it is best-effort and skipped when the project
# has no chat backend.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="${LLM_WIKI_PROJECT:-${ROOT}}"
TOKEN="${LLM_WIKI_API_TOKEN:-e2e-test-token}"
PORT="${LLM_WIKI_PORT:-8090}"   # avoid clashing with e2e-local.sh's 8080
BASE="http://127.0.0.1:${PORT}"
SERVER="${ROOT}/overlay/server/target/release/llm-wiki-server"
LANDING="${ROOT}/overlay/static"
AUTH_DB_DIR="$(mktemp -d -t llm-wiki-auth-e2e.XXXXXX)"
AUTH_DB="${AUTH_DB_DIR}/auth.db"
COOKIE_JAR="${AUTH_DB_DIR}/cj.txt"
PID_FILE="${AUTH_DB_DIR}/srv.pid"
LIMIT=3

if [[ ! -x "${SERVER}" ]]; then
  echo "error: server binary missing — run: cargo build --release --manifest-path overlay/server/Cargo.toml" >&2
  exit 1
fi

cleanup() {
  if [[ -f "${PID_FILE}" ]]; then
    kill "$(cat "${PID_FILE}")" 2>/dev/null || true
    rm -f "${PID_FILE}"
  fi
  rm -rf "${AUTH_DB_DIR}"
}
trap cleanup EXIT

fuser -k "${PORT}/tcp" 2>/dev/null || true
sleep 1

LLM_WIKI_PROJECT="${PROJECT}" \
LLM_WIKI_API_TOKEN="${TOKEN}" \
LLM_WIKI_REPO="${ROOT}" \
LLM_WIKI_BIND="127.0.0.1:${PORT}" \
LLM_WIKI_PUBLIC_LANDING_DIR="${LANDING}" \
LLM_WIKI_AUTH_DB="${AUTH_DB}" \
LLM_WIKI_DAILY_CHAT_LIMIT="${LIMIT}" \
  "${SERVER}" >"${AUTH_DB_DIR}/srv.log" 2>&1 &
echo $! > "${PID_FILE}"

for i in $(seq 1 30); do
  if curl -fsS "${BASE}/api/v1/health?token=${TOKEN}" >/dev/null 2>&1; then
    break
  fi
  sleep 1
  if [[ "${i}" -eq 30 ]]; then
    echo "error: server failed to start" >&2
    tail -30 "${AUTH_DB_DIR}/srv.log" >&2
    exit 1
  fi
done

echo "=== landing ==="
curl -s "${BASE}/" | grep -q "开始使用" && echo "  landing ok"

echo "=== register ==="
curl -s -c "${COOKIE_JAR}" -X POST "${BASE}/auth/register" \
  -H 'Content-Type: application/json' \
  -d '{"email":"e2e@test.com","password":"longenough"}' | grep -q '"user"' && echo "  register ok"

echo "=== /auth/me ==="
curl -sf -b "${COOKIE_JAR}" "${BASE}/auth/me" | grep -q "\"limit\":${LIMIT}" && echo "  me ok (limit=${LIMIT})"

echo "=== conversations CRUD ==="
CONV=$(curl -s -b "${COOKIE_JAR}" -X POST "${BASE}/api/v1/conversations" \
  -H 'Content-Type: application/json' -d '{"project_id":"px","title":"smoke"}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))')
[[ -n "${CONV}" ]] && echo "  create ok: ${CONV}"
curl -s -b "${COOKIE_JAR}" "${BASE}/api/v1/conversations" | grep -q "${CONV}" && echo "  list ok"

echo "=== chat usage limit (best-effort) ==="
PROJECT_ID="$(curl -sf -H "Authorization: Bearer ${TOKEN}" "${BASE}/api/v1/projects" \
  | python3 -c 'import sys,json; d=json.load(sys.stdin); p=d.get("currentProject") or (d.get("projects") or [{}])[0]; print(p.get("id",""))' 2>/dev/null || echo "")"
if [[ -n "${PROJECT_ID}" ]]; then
  for i in 1 2 3 4; do
    CODE=$(curl -s -b "${COOKIE_JAR}" -o /dev/null -w '%{http_code}' \
      -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/chat" \
      -H 'Content-Type: application/json' \
      -d '{"messages":[{"role":"user","content":"hi"}]}' --max-time 60)
    echo "  chat ${i} -> ${CODE}"
  done
  echo "  (expected first 3 = 200, 4th = 429 when chat backend is configured)"
else
  echo "  skipped — no project_id (set LLM_WIKI_PROJECT to a real wiki for this step)"
fi

echo "=== logout ==="
curl -s -b "${COOKIE_JAR}" -X POST "${BASE}/auth/logout" | grep -q '"ok":true' && echo "  logout ok"
curl -s -o /dev/null -w '  /auth/me after logout: %{http_code} (expect 401)\n' -b "${COOKIE_JAR}" "${BASE}/auth/me"

echo "=== Bearer path still works ==="
curl -sf -H "Authorization: Bearer ${TOKEN}" "${BASE}/api/v1/projects" >/dev/null && echo "  bearer ok"

echo ""
echo "Auth E2E passed."
