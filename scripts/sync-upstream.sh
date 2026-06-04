#!/usr/bin/env bash
# Upgrade the upstream git submodule to a tag or branch.
#
# Usage:
#   ./scripts/sync-upstream.sh              # stay on current checkout
#   ./scripts/sync-upstream.sh v0.4.20      # checkout tag
#   ./scripts/sync-upstream.sh main         # track upstream main
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UPSTREAM="${ROOT}/upstream"
REF="${1:-}"

if [[ ! -d "${UPSTREAM}/.git" ]]; then
  echo "error: upstream submodule not initialized. Run:"
  echo "  git submodule update --init --recursive"
  exit 1
fi

cd "${UPSTREAM}"
git fetch origin --tags

if [[ -n "${REF}" ]]; then
  if git rev-parse "refs/tags/${REF}" >/dev/null 2>&1; then
    git checkout "${REF}"
  elif git rev-parse "origin/${REF}" >/dev/null 2>&1; then
    git checkout -B "${REF}" "origin/${REF}"
  else
    echo "error: cannot resolve ref '${REF}' (not a tag or origin branch)"
    exit 1
  fi
fi

echo "upstream now at: $(git describe --tags --always 2>/dev/null || git rev-parse --short HEAD)"

cd "${ROOT}"
if command -v npm >/dev/null 2>&1 && [[ -f "${UPSTREAM}/package.json" ]]; then
  echo "running upstream mock tests..."
  npm install --prefix "${UPSTREAM}" 2>/dev/null || true
  npm run test:mocks --prefix "${UPSTREAM}" || {
    echo "warn: upstream test:mocks failed — review before committing submodule bump"
  }
fi

echo ""
echo "Next steps:"
echo "  cd ${ROOT} && git add upstream && git commit -m \"chore: bump upstream to ${REF:-$(cd upstream && git rev-parse --short HEAD)}\""
