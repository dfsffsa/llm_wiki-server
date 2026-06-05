#!/usr/bin/env bash
# Apply overlay patches to upstream submodule (idempotent via git apply --check).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UPSTREAM="${ROOT}/upstream"
PATCH_DIR="${ROOT}/overlay/patches"

if [[ ! -d "${UPSTREAM}/.git" && ! -f "${UPSTREAM}/.git" ]]; then
  echo "error: upstream submodule not initialized"
  exit 1
fi

shopt -s nullglob
patches=("${PATCH_DIR}"/*.patch)
if (( ${#patches[@]} == 0 )); then
  echo "No patches to apply."
  exit 0
fi

cd "${UPSTREAM}"
for patch in "${patches[@]}"; do
  name="$(basename "${patch}")"
  if git apply --reverse --check "${patch}" 2>/dev/null; then
    echo "already applied: ${name}"
    continue
  fi
  if git apply --check "${patch}" 2>/dev/null; then
    git apply "${patch}"
    echo "applied: ${name}"
  else
    echo "warning: patch does not apply cleanly: ${name}" >&2
    exit 1
  fi
done
