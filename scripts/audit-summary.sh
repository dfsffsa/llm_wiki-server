#!/usr/bin/env bash
# Summarize the in-house audit logs (access-*.jsonl) on a deployed server.
#
# Reads the JSONL files written by the server's audit module
# (LLM_WIKI_AUDIT_DIR) and aggregates per UTC day: request count, unique
# visitor IPs, most-hit path, and a 2xx/3xx/4xx/5xx status histogram. This is
# the in-house replacement for "how many people visited" — no Cloudflare
# dependency.
#
# Usage:
#   ./scripts/audit-summary.sh ecs99            # remote, default audit dir
#   ./scripts/audit-summary.sh ecs199           # remote, default audit dir
#   ./scripts/audit-summary.sh --local /path/to/audit-dir   # local (dev/tests)
#
# Env overrides (defaults match the real deployment):
#   SSH_HOME       dir holding ecs99-connect-22022.sh (default ~/cross-device-syncer/ssh-tunnels)
#   ECS99_SSH      ssh command for ecs99   (default "$SSH_HOME/ecs99-connect-22022.sh")
#   ECS99_AUDIT_DIR  default /root/llm_wiki-server/audit
#   ECS199_SSH     ssh command for ecs199  (default "ssh -F /tmp/ecs199-deploy.conf ecs199-server")
#   ECS199_AUDIT_DIR default /home/li/code/personal/llm_wiki-server/audit
set -euo pipefail

SSH_HOME="${SSH_HOME:-$HOME/cross-device-syncer/ssh-tunnels}"
ECS99_SSH="${ECS99_SSH:-$SSH_HOME/ecs99-connect-22022.sh}"
ECS99_AUDIT_DIR="${ECS99_AUDIT_DIR:-/root/llm_wiki-server/audit}"
ECS199_SSH="${ECS199_SSH:-ssh -F /tmp/ecs199-deploy.conf ecs199-server}"
ECS199_AUDIT_DIR="${ECS199_AUDIT_DIR:-/home/li/code/personal/llm_wiki-server/audit}"

# JSONL aggregator: one summary line per UTC day.
read -r -d '' AGG <<'PY' || true
import sys, json
from collections import defaultdict, Counter
days = defaultdict(lambda: {"n": 0, "ips": set(), "status": Counter(), "paths": Counter()})
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        r = json.loads(line)
    except Exception:
        continue
    d = r.get("ts", "")[:10]
    if not d:
        continue
    g = days[d]
    g["n"] += 1
    g["ips"].add(r.get("ip", ""))
    g["status"][r.get("status", 0)] += 1
    g["paths"][r.get("path", "")] += 1

def bucket(s):
    if s >= 500: return "5xx"
    if s >= 400: return "4xx"
    if s >= 300: return "3xx"
    return "2xx"

total_req = total_ips = 0
for d in sorted(days):
    g = days[d]
    top = g["paths"].most_common(1)[0] if g["paths"] else ("", 0)
    bc = Counter(bucket(s) for s in g["status"].elements())
    total_req += g["n"]
    total_ips += len(g["ips"])
    print(f'{d}  req={g["n"]:<6} uniq_ips={len(g["ips"]):<5} top={top[0]} ({top[1]}x)  ' +
          " ".join(f'{k}={bc[k]}' for k in ("2xx", "3xx", "4xx", "5xx") if bc[k]))
if days:
    print(f'TOTAL  req={total_req:<6} uniq_ips(sum of days)={total_ips}')
PY

mode="${1:?usage: audit-summary.sh ecs99|ecs199|--local <dir>}"
case "$mode" in
  ecs99)
    echo "===== ecs99 (cn.sship.online) — $ECS99_AUDIT_DIR ====="
    "$ECS99_SSH" "cat '$ECS99_AUDIT_DIR'/access-*.jsonl 2>/dev/null" | python3 -c "$AGG"
    ;;
  ecs199)
    echo "===== ecs199 (glb.sship.online) — $ECS199_AUDIT_DIR ====="
    # shellcheck disable=SC2086  # ECS199_SSH is a command string, split intentionally
    $ECS199_SSH "cat '$ECS199_AUDIT_DIR'/access-*.jsonl 2>/dev/null" | python3 -c "$AGG"
    ;;
  --local)
    local_dir="${2:?usage: audit-summary.sh --local <dir>}"
    echo "===== local — $local_dir ====="
    cat "$local_dir"/access-*.jsonl 2>/dev/null | python3 -c "$AGG"
    ;;
  *)
    echo "unknown mode '$mode'; use ecs99 | ecs199 | --local <dir>" >&2
    exit 2
    ;;
esac
