#!/usr/bin/env bash
# Hot-backup the auth SQLite database (users / sessions / conversations /
# usage) to a timestamped file, then prune old backups.
#
# Uses `sqlite3 DB ".backup DEST"` — an online backup that is safe to run
# while the server is writing (it uses SQLite's backup API, not a raw file
# copy, so it produces a consistent snapshot even mid-transaction). The
# server keeps running; no lock, no downtime.
#
# Usage:
#   ./scripts/backup-auth-db.sh /path/to/auth.db /var/backups/llm-wiki 14
#
# Env overrides:
#   AUTH_DB        — path to the auth SQLite file (arg 1 fallback)
#   BACKUP_DIR     — destination directory (arg 2 fallback)
#   KEEP_COPIES    — number of recent backups to keep (arg 3 fallback, default 14)
#
# Cron example (daily at 03:17):
#   17 3 * * *  /opt/llm-wiki-server/scripts/backup-auth-db.sh >> /var/log/llm-wiki-backup.log 2>&1
#
# Recovery: stop the server, replace the auth db with a backup copy, restart:
#   systemctl stop llm-wiki-server
#   cp /var/backups/llm-wiki/auth-2026-06-26.db /path/to/auth.db
#   systemctl start llm-wiki-server

set -euo pipefail

AUTH_DB="${1:-${AUTH_DB:-}}"
BACKUP_DIR="${2:-${BACKUP_DIR:-}}"
KEEP_COPIES="${3:-${KEEP_COPIES:-14}}"

if [[ -z "$AUTH_DB" || -z "$BACKUP_DIR" ]]; then
    echo "usage: $0 <auth-db-path> <backup-dir> [keep-copies]" >&2
    echo "  (or set AUTH_DB / BACKUP_DIR env)" >&2
    exit 2
fi
if ! command -v sqlite3 >/dev/null 2>&1; then
    echo "error: sqlite3 CLI not found (apt install sqlite3)" >&2
    exit 3
fi
if [[ ! -f "$AUTH_DB" ]]; then
    echo "error: auth db not found: $AUTH_DB" >&2
    exit 2
fi

mkdir -p "$BACKUP_DIR"
STAMP="$(date -u +%Y-%m-%dT%H%M%SZ)"
DEST="$BACKUP_DIR/auth-$STAMP.db"

# `.backup` is the online hot-backup path — consistent snapshot, safe under
# concurrent writes. `.timeout` adds a short busy-wait if a write txn is open.
echo "[backup] $AUTH_DB -> $DEST"
sqlite3 "$AUTH_DB" ".timeout 5000" ".backup '$DEST'"

# Integrity-check the backup; a corrupt backup is worse than none.
if ! sqlite3 "$DEST" "PRAGMA integrity_check;" >/dev/null 2>&1; then
    echo "[backup] WARNING: integrity_check failed on $DEST; removing" >&2
    rm -f "$DEST"
    exit 1
fi

# Compress to save space (auth db is small; gzip is fine).
gzip -f "$DEST"
echo "[backup] wrote $DEST.gz ($(du -h "$DEST.gz" | cut -f1))"

# Prune: keep the newest KEEP_COPIES *.db.gz files.
shopt -s nullglob
mapfile -t BACKUPS< <(ls -1t "$BACKUP_DIR"/auth-*.db.gz 2>/dev/null)
if (( ${#BACKUPS[@]} > KEEP_COPIES )); then
    for f in "${BACKUPS[@]:KEEP_COPIES}"; do
        echo "[backup] prune $f"
        rm -f "$f"
    done
fi

echo "[backup] done; ${#BACKUPS[@]} backup(s) retained (cap $KEEP_COPIES)"
