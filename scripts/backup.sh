#!/usr/bin/env bash
# Backs up a Saldo running with docker compose: a consistent copy of the
# database (`saldo backup`, safe while the server runs) and a tarball of the
# attachment files, both written to a folder on the host. Old backups beyond
# KEEP_DAYS are removed.
#
# Usage: scripts/backup.sh /path/to/backups      (run from the compose folder)
# Cron:  15 3 * * * cd /opt/saldo && scripts/backup.sh /srv/backups/saldo >> /var/log/saldo-backup.log 2>&1
set -euo pipefail

DEST="${1:?usage: scripts/backup.sh <backup folder>}"
KEEP_DAYS="${KEEP_DAYS:-14}"
SERVICE="${SALDO_SERVICE:-saldo}"
STAMP="$(date +%F-%H%M)"
TMP="/data/backup-$STAMP.db"

mkdir -p "$DEST"
docker compose exec -T "$SERVICE" saldo backup "$TMP"
docker compose cp "$SERVICE:$TMP" "$DEST/saldo-$STAMP.db"
docker compose exec -T "$SERVICE" rm -f "$TMP"
# The folder exists only after the first upload; an empty one still archives.
docker compose exec -T "$SERVICE" sh -c 'mkdir -p /data/attachments && tar -C /data -czf - attachments' \
    >"$DEST/attachments-$STAMP.tar.gz"
find "$DEST" -maxdepth 1 \( -name 'saldo-*.db' -o -name 'attachments-*.tar.gz' \) -mtime +"$KEEP_DAYS" -delete
echo "backup written to $DEST (saldo-$STAMP.db, attachments-$STAMP.tar.gz)"
