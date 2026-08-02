#!/usr/bin/env bash
# E2E smoke test for cookie auth + history + usage limit.
# Starts a fresh server with auth DB, runs full CRUD cycle, then cleans up.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SERVER="${ROOT}/overlay/server/target/release/llm-wiki-server"
PROJECT="${LLM_WIKI_PROJECT:-${ROOT}/../llm_wiki_projects/CivilCareer}"
TOKEN="${LLM_WIKI_API_TOKEN:-e2e-test-token}"
PORT=8567
BASE="http://127.0.0.1:${PORT}"
STATIC="${ROOT}/upstream/dist"
CONFIG="${ROOT}/overlay/config/server.minimax.local.json"
TMPDIR="/tmp/llm-wiki-e2e-auth"
PID_FILE="${TMPDIR}/server.pid"
PASS=0; FAIL=0

cleanup() {
  if [[ -f "${PID_FILE}" ]]; then kill "$(cat "${PID_FILE}")" 2>/dev/null || true; rm -f "${PID_FILE}"; fi
  rm -rf "${TMPDIR}"
}
trap cleanup EXIT

mkdir -p "${TMPDIR}" && rm -f "${TMPDIR}"/*

echo "==> Starting server..."
LLM_WIKI_PROJECT="${PROJECT}" \
LLM_WIKI_API_TOKEN="${TOKEN}" \
LLM_WIKI_BIND="127.0.0.1:${PORT}" \
LLM_WIKI_STATIC="${STATIC}" \
LLM_WIKI_CONFIG="${CONFIG}" \
LLM_WIKI_REPO="${ROOT}" \
LLM_WIKI_AUTH_DB="${TMPDIR}/auth.db" \
LLM_WIKI_DAILY_CHAT_LIMIT=3 \
LLM_WIKI_PUBLIC_LANDING_DIR="${ROOT}/overlay/static" \
  "${SERVER}" > "${TMPDIR}/server.log" 2>&1 &
echo $! > "${PID_FILE}"

for i in $(seq 1 15); do
  if curl -fsS "${BASE}/api/v1/health?token=${TOKEN}" >/dev/null 2>&1; then break; fi
  sleep 1
  if [[ "${i}" -eq 15 ]]; then echo "error: server failed to start" >&2; tail -10 "${TMPDIR}/server.log" >&2; exit 1; fi
done

ok() { PASS=$((PASS+1)); echo "  ✅ $1"; }
fail() { FAIL=$((FAIL+1)); echo "  ❌ $1: $2"; }

echo; echo "==> Landing page"
curl -sS "${BASE}/" | grep -q "免费注册" && ok "landing page" || fail "landing page" "no 免费注册"

echo; echo "==> Register"
REG=$(curl -sS -c "${TMPDIR}/cookies.txt" -X POST "${BASE}/auth/register" -H 'Content-Type: application/json' -d '{"email":"e2e@test.com","password":"longenough"}') || true
echo "${REG}" | grep -q '"ok":true' && ok "register (email sent)" || fail "register" "no ok:true"

echo; echo "==> Verify email (token from server log, SMTP not configured)"
VT=$(grep 'verification token for e2e@test.com' "${TMPDIR}/server.log" | sed -E 's/.*verification token for e2e@test.com: ([^ ]*).*/\1/' | tail -1)
VC=$(curl -sS -o /dev/null -w '%{http_code}' "${BASE}/auth/verify-email?token=${VT}")
[[ "${VC}" == "302" ]] && ok "verify-email -> 302" || fail "verify-email" "got ${VC}"

echo; echo "==> Login (fresh session for verified user)"
LC=$(curl -sS -c "${TMPDIR}/cookies.txt" -o /dev/null -w '%{http_code}' -X POST "${BASE}/auth/login" -H 'Content-Type: application/json' -d '{"email":"e2e@test.com","password":"longenough"}')
[[ "${LC}" == "200" ]] && ok "login" || fail "login" "got ${LC}"

echo; echo "==> /auth/me with cookie"
ME=$(curl -sS -b "${TMPDIR}/cookies.txt" "${BASE}/auth/me")
echo "${ME}" | grep -q '"limit":3' && ok "/auth/me (limit=3)" || fail "/auth/me" "no limit=3"

echo; echo "==> /auth/me without cookie (expect 401)"
CODE=$(curl -sS -o /dev/null -w '%{http_code}' "${BASE}/auth/me")
[[ "${CODE}" == "401" ]] && ok "/auth/me no cookie -> 401" || fail "/auth/me no cookie" "got ${CODE}"

echo; echo "==> Conversations CRUD"
CONV=$(curl -sS -b "${TMPDIR}/cookies.txt" -X POST "${BASE}/api/v1/conversations" -H 'Content-Type: application/json' -d '{"project_id":"px","title":"e2e"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])' 2>/dev/null) || CONV=""
[[ -n "${CONV}" ]] && ok "create conversation" || fail "create conversation" "no id"
curl -sS -b "${TMPDIR}/cookies.txt" -X POST "${BASE}/api/v1/conversations/${CONV}/messages" -H 'Content-Type: application/json' -d '{"role":"user","content":"hi"}' >/dev/null 2>&1 || true
curl -sS -b "${TMPDIR}/cookies.txt" -X POST "${BASE}/api/v1/conversations/${CONV}/messages" -H 'Content-Type: application/json' -d '{"role":"assistant","content":"hello"}' >/dev/null 2>&1 || true
MSGS=$(curl -sS -b "${TMPDIR}/cookies.txt" "${BASE}/api/v1/conversations/${CONV}/messages")
echo "${MSGS}" | grep -q '"hi"' && ok "message append+list" || fail "message append+list" "no hi"

echo; echo "==> Chat usage limit (limit=3)"
PROJECT_ID=$(curl -sS -H "Authorization: Bearer ${TOKEN}" "${BASE}/api/v1/projects" | python3 -c 'import sys,json;print(json.load(sys.stdin)["projects"][0]["id"])' 2>/dev/null) || PROJECT_ID=""
OK_COUNT=0
for i in 1 2 3 4; do
  CODE=$(curl -sS -b "${TMPDIR}/cookies.txt" -o /dev/null -w '%{http_code}' -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/chat" -H 'Content-Type: application/json' -d '{"messages":[{"role":"user","content":"hi"}]}' --max-time 60 2>/dev/null || echo "000")
  if [[ "${CODE}" == "200" ]]; then OK_COUNT=$((OK_COUNT+1)); fi
  [[ "${CODE}" == "429" ]] && echo "  chat $i -> 429 (expected)"
done
[[ "${OK_COUNT}" -eq 3 ]] && ok "chat quota" || fail "chat quota" "expected 3x200 got ${OK_COUNT}x200"

echo; echo "==> Bearer bypasses quota"
BEARER_CODE=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/chat" -H "Authorization: Bearer ${TOKEN}" -H 'Content-Type: application/json' -d '{"messages":[{"role":"user","content":"hi"}]}' --max-time 60 2>/dev/null || echo "000")
[[ "${BEARER_CODE}" == "200" ]] && ok "bearer bypass quota" || fail "bearer bypass" "got ${BEARER_CODE}"

echo; echo "==> Logout"
LOG=$(curl -sS -b "${TMPDIR}/cookies.txt" -X POST "${BASE}/auth/logout")
echo "${LOG}" | grep -q '"ok":true' && ok "logout" || fail "logout" "no ok:true"

echo; echo "==> /auth/me after logout (expect 401)"
CODE=$(curl -sS -o /dev/null -w '%{http_code}' -b "${TMPDIR}/cookies.txt" "${BASE}/auth/me")
[[ "${CODE}" == "401" ]] && ok "me after logout -> 401" || fail "me after logout" "got ${CODE}"

echo; echo "==> Re-login + forgot"
curl -sS -c "${TMPDIR}/c2.txt" -X POST "${BASE}/auth/login" -H 'Content-Type: application/json' -d '{"email":"e2e@test.com","password":"longenough"}' >/dev/null 2>&1
FORGOT=$(curl -sS -o /dev/null -w '%{http_code}' -X POST "${BASE}/auth/forgot-password" -H 'Content-Type: application/json' -d '{"email":"e2e@test.com"}')
[[ "${FORGOT}" == "200" ]] && ok "forgot-password" || fail "forgot-password" "got ${FORGOT}"

echo; echo "==> All auth pages serve"
for path in "/login" "/register" "/reset-password"; do
  CODE=$(curl -sS -o /dev/null -w '%{http_code}' "${BASE}${path}")
  [[ "${CODE}" == "200" ]] && ok "${path} -> 200" || fail "${path}" "got ${CODE}"
done

echo; echo "==> Bearer projects (regression)"
CODE=$(curl -sS -o /dev/null -w '%{http_code}' -H "Authorization: Bearer ${TOKEN}" "${BASE}/api/v1/projects")
[[ "${CODE}" == "200" ]] && ok "bearer projects" || fail "bearer projects" "got ${CODE}"

echo; echo "================================================"
echo "Results: ${PASS} passed, ${FAIL} failed"
echo "================================================"
[[ "${FAIL}" -eq 0 ]] && exit 0 || exit 1
