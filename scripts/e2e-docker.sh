#!/usr/bin/env bash
# End-to-end smoke test: Docker deploy + HTTP API against a real wiki project.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMPOSE="${ROOT}/docker/docker-compose.yml"
PROJECT="${LLM_WIKI_PROJECT:-${ROOT}/../llm_wiki_projects/CivilCareer}"
TOKEN="${LLM_WIKI_API_TOKEN:-e2e-test-token}"
PORT="${LLM_WIKI_PORT:-8080}"
BASE="http://127.0.0.1:${PORT}"

if [[ ! -d "${PROJECT}/wiki" ]]; then
  echo "error: wiki project not found (expected wiki/ under ${PROJECT})" >&2
  exit 1
fi

DOCKER=(docker)
USE_WIN_ENV=0
if [[ -x "/mnt/c/Program Files/Docker/Docker/resources/bin/docker.exe" ]]; then
  DOCKER=("/mnt/c/Program Files/Docker/Docker/resources/bin/docker.exe")
  USE_WIN_ENV=1
elif ! command -v docker >/dev/null 2>&1; then
  echo "error: docker not found" >&2
  exit 1
fi

docker_volume_path() {
  local p="$1"
  p="$(cd "$(dirname "$p")" && pwd)/$(basename "$p")"
  if [[ "${USE_WIN_ENV}" -eq 1 && -n "${WSL_DISTRO_NAME:-}" ]]; then
    printf '//wsl.localhost/%s%s' "${WSL_DISTRO_NAME}" "${p}"
  else
    printf '%s' "${p}"
  fi
}

ENV_FILE="${ROOT}/docker/.env.e2e.local"
PROJECT_VOL="$(docker_volume_path "${PROJECT}")"
cat > "${ENV_FILE}" <<EOF
LLM_WIKI_PROJECT=${PROJECT_VOL}
LLM_WIKI_API_TOKEN=${TOKEN}
LLM_WIKI_PORT=${PORT}
EOF

COMPOSE_ENV=(--env-file "${ENV_FILE}")
if [[ "${USE_WIN_ENV}" -eq 1 ]]; then
  COMPOSE_ENV=(--env-file "$(wslpath -w "${ENV_FILE}")")
fi

compose() {
  "${DOCKER[@]}" compose "${COMPOSE_ENV[@]}" -f "${COMPOSE}" "$@"
}

cleanup() {
  compose down --remove-orphans >/dev/null 2>&1 || true
  rm -f "${ENV_FILE}"
}
trap cleanup EXIT

echo "==> Wiki project: ${PROJECT}"
echo "==> Docker volume: ${PROJECT_VOL}"
echo "==> Building image (this may take several minutes on first run)..."
compose up --build -d

echo "==> Waiting for health..."
for i in $(seq 1 60); do
  if curl -fsS "${BASE}/api/v1/health?token=${TOKEN}" >/dev/null 2>&1; then
    break
  fi
  sleep 3
  if [[ "${i}" -eq 60 ]]; then
    echo "error: server did not become healthy" >&2
    compose logs --tail=80
    exit 1
  fi
done

echo "==> API checks"
curl -fsS "${BASE}/api/v1/health?token=${TOKEN}" | python3 -m json.tool | head -20

PROJECTS_JSON="$(curl -fsS "${BASE}/api/v1/projects?token=${TOKEN}")"
echo "${PROJECTS_JSON}" | python3 -m json.tool | head -30
PROJECT_ID="$(echo "${PROJECTS_JSON}" | python3 -c "import json,sys; d=json.load(sys.stdin); p=d.get('currentProject') or (d.get('projects') or [None])[0]; print(p['id'] if p else '')")"
if [[ -z "${PROJECT_ID}" ]]; then
  echo "error: no project id from /projects" >&2
  exit 1
fi
echo "project id: ${PROJECT_ID}"

echo "==> Read wiki/index.md"
curl -fsS "${BASE}/api/v1/projects/${PROJECT_ID}/files/content?token=${TOKEN}&path=wiki/index.md" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('content','')[:400])"

echo "==> Search (keyword)"
curl -fsS -X POST "${BASE}/api/v1/projects/${PROJECT_ID}/search?token=${TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{"query":"职场","topK":5}' \
  | python3 -m json.tool | head -40

echo "==> Static UI"
HTML="$(curl -fsS "${BASE}/")"
echo "${HTML}" | head -5
echo "${HTML}" | grep -q '<div id="root">' && echo "UI root element: ok"

echo ""
echo "E2E passed. Open in browser: ${BASE}/"
echo "API token ${TOKEN} is embedded in the UI bundle at build time."
