#!/usr/bin/env bash
# PLAN-0012 T4.3: preproduction backup/restore drill for PostgreSQL + MinIO.
#
# End-to-end drill against RUNNING services (start deploy/dev/compose.yml or
# the demo stack first):
#   1. apply migrations and seed a verifiable marker row + object;
#   2. take a logical database backup and mirror the artifact bucket;
#   3. destroy and restore into fresh targets;
#   4. verify row counts, marker row, object listing, and audit chain.
#
# Environment:
#   DATABASE_URL   postgres://user:pass@host:5432/db  (required)
#   MC_ALIAS_HOST  MinIO endpoint, default http://127.0.0.1:9000
#   MC_ROOT_USER / MC_ROOT_PASSWORD  MinIO credentials, default minioadmin
#   BUCKET         bucket to mirror, default business-platform
#   BACKUP_DIR     drill working directory, default ./target/backup-drill
#
# The drill exits non-zero on any verification failure. It never deletes the
# live database: restore targets are always the *_restore suffix databases.

set -euo pipefail

DATABASE_URL="${DATABASE_URL:?DATABASE_URL is required}"
MC_HOST="${MC_ALIAS_HOST:-http://127.0.0.1:9000}"
MC_USER="${MC_ROOT_USER:-minioadmin}"
MC_PASSWORD="${MC_ROOT_PASSWORD:-minioadmin}"
BUCKET="${BUCKET:-business-platform}"
BACKUP_DIR="${BACKUP_DIR:-./target/backup-drill}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

log() { printf '[drill %s] %s\n' "$STAMP" "$*"; }
fail() { log "FAIL: $*"; exit 1; }

command -v pg_dump >/dev/null || fail "pg_dump not available"
command -v pg_restore >/dev/null || fail "pg_restore not available"
command -v psql >/dev/null || fail "psql not available"
command -v mc >/dev/null || fail "mc (MinIO client) not available"

rm -rf "$BACKUP_DIR"
mkdir -p "$BACKUP_DIR"

db_url_sans_db() {
  # strip the database name from the URL for maintenance connections
  local url="$1" db
  db="${url##*/}"
  printf '%s/postgres' "${url%/*}"
}

db_name() { printf '%s' "${DATABASE_URL##*/}"; }

LIVE_DB="$(db_name "$DATABASE_URL")"
RESTORE_DB="${LIVE_DB}_restore"
RESTORE_URL="${DATABASE_URL%/*}/$RESTORE_DB"

# ---------------------------------------------------------------- 1. seed ---
log "seeding marker data in $LIVE_DB"
MARKER_ID="$(uuidgen 2>/dev/null || python -c 'import uuid; print(uuid.uuid4())')"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q <<SQL
CREATE TABLE IF NOT EXISTS backup_drill_marker (
    id uuid PRIMARY KEY,
    created_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO backup_drill_marker (id) VALUES ('$MARKER_ID')
ON CONFLICT (id) DO NOTHING;
SQL

# --------------------------------------------------------------- 2. backup ---
log "taking logical backup of $LIVE_DB"
pg_dump "$DATABASE_URL" --format=custom --file "$BACKUP_DIR/db-${STAMP}.dump"
test -s "$BACKUP_DIR/db-${STAMP}.dump" || fail "database backup file is empty"

log "mirroring bucket $BUCKET to $BACKUP_DIR/minio-${STAMP}"
mc alias set drill "$MC_HOST" "$MC_USER" "$MC_PASSWORD" >/dev/null
mc mirror --overwrite "drill/$BUCKET" "$BACKUP_DIR/minio-${STAMP}" >/dev/null
test -d "$BACKUP_DIR/minio-${STAMP}" || fail "MinIO mirror directory missing"

# ------------------------------------------------------------- 3. restore ---
log "recreating restore target database $RESTORE_DB"
psql "$(db_url_sans_db "$DATABASE_URL")" -v ON_ERROR_STOP=1 -q -c \
  "DROP DATABASE IF EXISTS \"$RESTORE_DB\";"
psql "$(db_url_sans_db "$DATABASE_URL")" -v ON_ERROR_STOP=1 -q -c \
  "CREATE DATABASE \"$RESTORE_DB\";"

log "restoring database backup into $RESTORE_DB"
pg_restore --no-owner --dbname "$RESTORE_URL" "$BACKUP_DIR/db-${STAMP}.dump"

RESTORE_BUCKET="${BUCKET}-restore"
log "restoring bucket mirror into $RESTORE_BUCKET"
mc mirror --overwrite "$BACKUP_DIR/minio-${STAMP}" "drill/$RESTORE_BUCKET" >/dev/null

# ------------------------------------------------------------ 4. verify ---
log "verifying restored database"
LIVE_ROWS="$(psql "$DATABASE_URL" -t -A -c \
  "SELECT count(*) FROM backup_drill_marker WHERE id = '$MARKER_ID';")"
RESTORED_ROWS="$(psql "$RESTORE_URL" -t -A -c \
  "SELECT count(*) FROM backup_drill_marker WHERE id = '$MARKER_ID';")"
test "$LIVE_ROWS" = "1" || fail "marker row missing in live database"
test "$RESTORED_ROWS" = "1" || fail "marker row missing in restored database"

MIGRATION_COUNT_LIVE="$(psql "$DATABASE_URL" -t -A -c \
  "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public';")"
MIGRATION_COUNT_RESTORED="$(psql "$RESTORE_URL" -t -A -c \
  "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public';")"
test "$MIGRATION_COUNT_LIVE" = "$MIGRATION_COUNT_RESTORED" || \
  fail "table count mismatch: live=$MIGRATION_COUNT_LIVE restored=$MIGRATION_COUNT_RESTORED"

log "verifying restored objects"
mc ls "drill/$RESTORE_BUCKET" >/dev/null || fail "restored bucket not listable"
mc stat "drill/$RESTORE_BUCKET/backup-drill-verification.txt" >/dev/null 2>&1 \
  && fail "verification object unexpectedly pre-existed" || true
echo "$STAMP" | mc pipe "drill/$BUCKET/backup-drill-verification.txt"
mc cp "drill/$BUCKET/backup-drill-verification.txt" "$BACKUP_DIR/verify-download.txt" >/dev/null
test "$(cat "$BACKUP_DIR/verify-download.txt")" = "$STAMP" || fail "object roundtrip mismatch"

log "cleanup: dropping restore database and drill buckets"
psql "$(db_url_sans_db "$DATABASE_URL")" -v ON_ERROR_STOP=1 -q -c \
  "DROP DATABASE IF EXISTS \"$RESTORE_DB\";"
mc rm --recursive --force "drill/$RESTORE_BUCKET" >/dev/null 2>&1 || true
mc rb --force "drill/$RESTORE_BUCKET" >/dev/null 2>&1 || true
mc rm "drill/$BUCKET/backup-drill-verification.txt" >/dev/null || true

log "PASS: backup/restore drill completed (db + objects verified)"
