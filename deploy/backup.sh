#!/usr/bin/env bash
# Dumps the Venti database to a file and removes dumps that are too old.
#
#     deploy/backup.sh
#
# Run it from anywhere; it works on the Compose project in its own folder.
# Each run writes <backup dir>/venti-<UTC time>.dump in PostgreSQL's custom
# format, which pg_restore reads (see README.md, "Restore"). Dumps older than
# the retention are deleted, but only after the new dump has been checked,
# so a failing backup never eats the good ones.
#
# Settings, from the environment or from the file .env beside this script:
#
#     VENTI_BACKUP_DIR        where dumps go        (default: ./backups)
#     VENTI_BACKUP_KEEP_DAYS  days a dump is kept   (default: 14)
#
# The database is everything that cannot be made again: pilots' accounts and
# their flight files are in it, beside the collected observations. So one
# dump is a complete backup, and each dump holds personal data and password
# hashes: the backup folder is readable by its owner only.
#
# The dumps are on the same machine as the database. That protects against a
# bad migration or a mistaken delete, not against losing the machine: copy
# the backup folder somewhere else as well.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

# Reads one setting from .env without running the file as a script.
setting() {
    local name="$1" value=""
    if [ -f .env ]; then
        value="$(sed -n "s/^${name}=//p" .env | tail -n 1)"
    fi
    printf '%s' "$value"
}

backup_dir="${VENTI_BACKUP_DIR:-$(setting VENTI_BACKUP_DIR)}"
backup_dir="${backup_dir:-$here/backups}"
keep_days="${VENTI_BACKUP_KEEP_DAYS:-$(setting VENTI_BACKUP_KEEP_DAYS)}"
keep_days="${keep_days:-14}"

case "$keep_days" in
    '' | *[!0-9]*)
        echo "VENTI_BACKUP_KEEP_DAYS must be a whole number, not '$keep_days'" >&2
        exit 2
        ;;
esac

mkdir -p "$backup_dir"
# The dump holds everything in the database; keep it to the owner.
chmod 700 "$backup_dir"

name="venti-$(date -u +%Y%m%dT%H%M%SZ).dump"
partial="$backup_dir/.$name.partial"
trap 'rm -f "$partial"' EXIT

# -T: no terminal, so the dump arrives byte for byte.
docker compose exec -T db \
    pg_dump --username venti --dbname venti --format custom --no-owner \
    > "$partial"

# A dump that pg_restore cannot list is not a backup.
docker compose exec -T db pg_restore --list < "$partial" > /dev/null

mv "$partial" "$backup_dir/$name"
echo "wrote $backup_dir/$name ($(du -h "$backup_dir/$name" | cut -f1))"

find "$backup_dir" -maxdepth 1 -name 'venti-*.dump' -mtime "+$keep_days" -print -delete \
    | sed 's/^/removed /'
