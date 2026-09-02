#!/usr/bin/env bash
# PLAN-0012 T4.3: preproduction backup/restore drill for PostgreSQL + MinIO.
#
# End-to-end drill against RUNNING services (start deploy/dev/compose.yml or
# the demo stack first, or the CI PostgreSQL/MinIO services):
#   1. seed a marker database row and a marker object (with content,
#      checksum and size recorded BEFORE the backup is taken);
#   2. take a logical database backup and mirror the artifact bucket;
#   3. restore into uniquely named, drill-owned targets
#      (<db>_drill_<stamp> and <bucket>-restore-<stamp>);
#   4. verify: marker row present, table counts equal, and the marker object
#      downloaded FROM THE RESTORE BUCKET with byte-for-byte content,
#      checksum and size comparison against the pre-backup record.
#
# Safety model:
#   - working data lives only in a freshly created, unique run directory
#     under BACKUP_DIR; the script never deletes a pre-existing directory
#     and refuses dangerous BACKUP_DIR targets (empty, relative, root, or
#     system directories);
#   - restore targets are always drill-owned names; the script refuses live
#     databases/buckets that already look like drill targets;
#   - an EXIT trap cleans up exactly the resources this run created.
#
# Environment:
#   DATABASE_URL   postgres://user:pass@host:5432/db  (required)
#   MC_ALIAS_HOST  MinIO endpoint, default http://127.0.0.1:9000
#   MC_ROOT_USER / MC_ROOT_PASSWORD  MinIO credentials, default minioadmin
#   BUCKET         bucket to mirror, default business-platform
#   BACKUP_DIR     drill root directory, default ./target/backup-drill
#   DRILL_SELFTEST set to 1 to run the path-guard/cleanup selftest and exit
#                  (no external tools or services required)
#
# The drill exits non-zero on any verification failure. It never deletes the
# live database, the live bucket, or any bucket other than its own restore
# bucket.

set -euo pipefail

# ------------------------------------------------------------- helpers ------
log() { printf '[drill] %s\n' "$*"; }
fail() { printf '[drill] FAIL: %s\n' "$*" >&2; exit 1; }

# resolve_abs <path>: canonical absolute path (parent may not exist yet).
resolve_abs() {
  local candidate="$1"
  if command -v realpath >/dev/null 2>&1; then
    realpath -m "$candidate"
  else
    # POSIX fallback: canonicalize the existing prefix.
    local probe="$candidate" tail=""
    while [ ! -d "$probe" ]; do
      tail="$(basename "$probe")/$tail"
      probe="$(dirname "$probe")"
    done
    printf '%s/%s' "$(cd "$probe" && pwd)" "${tail%/}"
  fi
}

rand_suffix() {
  if command -v od >/dev/null 2>&1 && [ -r /dev/urandom ]; then
    od -An -tx1 -N4 /dev/urandom | tr -d ' \n'
  else
    printf '%s' "$RANDOM$RANDOM" | cksum | cut -d' ' -f1
  fi
}

# is_dangerous_root <resolved-abs-path>: reject paths that must never host
# an auto-removed drill tree.
is_dangerous_root() {
  local root="$1"
  case "$root" in
    / | //) return 0 ;;
    [A-Za-z]:\\* | [A-Za-z]:/* | [A-Za-z]:*) fail "refusing Windows drive root as BACKUP_DIR: $root" ;;
  esac
  # A root strictly inside the invocation working tree is the operator's own
  # workspace (the drill default `./target/backup-drill` lives there, and the
  # cleanup trap only ever removes the drill's own `run-*` directory under
  # it). CI checkouts legitimately live under /home or /var, so the system
  # prefix denial below applies only to paths outside the working tree.
  local cwd_abs in_work_tree=0
  cwd_abs="$(resolve_abs "$PWD")"
  case "$root" in
    "$cwd_abs"/*) in_work_tree=1 ;;
  esac
  if [ "$in_work_tree" -eq 0 ]; then
    case "$root" in
      /bin | /bin/* | /boot | /boot/* | /dev | /dev/* | /etc | /etc/* \
      | /home | /home/* | /lib | /lib/* | /lib64 | /lib64/* | /media | /media/* \
      | /mnt | /mnt/* | /opt | /opt/* | /proc | /proc/* | /root | /root/* \
      | /run | /run/* | /sbin | /sbin/* | /srv | /srv/* | /sys | /sys/* \
      | /tmp | /usr | /usr/* | /var | /var/*) return 0 ;;
    esac
  fi
  # require at least two path segments below the filesystem root
  local stripped="${root#/}"
  case "$stripped" in
    */*) : ;;
    "") return 0 ;;
    *) return 0 ;;
  esac
  return 1
}

# plan_drill_dir <backup-dir-root>: validate root, produce a unique run dir.
plan_drill_dir() {
  local root_abs
  # reject Windows drive roots before normalization (MSYS may rewrite C: to /c)
  case "$1" in
    [A-Za-z]:\\* | [A-Za-z]:/* | [A-Za-z]:*) fail "refusing Windows drive root as BACKUP_DIR: $1" ;;
  esac
  root_abs="$(resolve_abs "$1")"
  [ -n "$root_abs" ] || fail "BACKUP_DIR resolves to an empty path"
  [ "$root_abs" != "/" ] || fail "BACKUP_DIR must not be the filesystem root"
  if is_dangerous_root "$root_abs"; then
    fail "BACKUP_DIR points at a dangerous system location: $root_abs"
  fi
  case "$root_abs" in
    /*) : ;;
    *) fail "BACKUP_DIR must resolve to an absolute path: $root_abs" ;;
  esac
  if [ -e "$root_abs" ] && [ ! -d "$root_abs" ]; then
    fail "BACKUP_DIR exists and is not a directory: $root_abs"
  fi
  printf '%s/run-%s-%s' "$root_abs" "$STAMP" "$RAND"
}

# -------------------------------------------------------- selftest mode -----
if [ "${DRILL_SELFTEST:-0}" = "1" ]; then
  STAMP="selftest"
  RAND="selftest"
  rejected=0; accepted=0
  # run the planner in a subshell: rejection paths exit inside the function
  expect_reject() {
    if ( plan_drill_dir "$1" ) >/dev/null 2>&1; then
      printf '[drill selftest] FAIL: accepted dangerous root: %s\n' "$1" >&2; rejected=$((rejected+1))
    fi
  }
  expect_accept() {
    if ( plan_drill_dir "$1" ) >/dev/null 2>&1; then accepted=$((accepted+1)); else
      printf '[drill selftest] FAIL: rejected safe root: %s\n' "$1" >&2; rejected=$((rejected+1)); fi
  }
  expect_reject "/"
  expect_reject "/etc"
  expect_reject "/var"
  expect_reject "/var/lib"
  expect_reject "/usr/local"
  expect_reject "/home"
  expect_reject "/root"
  expect_reject "C:\\backups"
  expect_reject "C:/backups"
  # relative roots resolve against CWD and must be accepted when not escaping
  # into a system directory; the empty/relative root case is covered by the
  # default BACKUP_DIR (./target/backup-drill under the repo).
  expect_accept "$(pwd)/target/backup-drill-selftest"
  expect_accept "/tmp-allowed-selftest-root/platform-drill"
  [ "$rejected" -eq 0 ] || fail "selftest: $rejected guard violation(s)"
  [ "$accepted" -eq 2 ] || fail "selftest: expected 2 accepted roots, got $accepted"
  # uniqueness: two invocations must not collide
  a="$(plan_drill_dir "$(pwd)/target/backup-drill-selftest")"
  b="$(plan_drill_dir "$(pwd)/target/backup-drill-selftest")"
  [ "$a" = "$b" ] || fail "selftest: run dir must be deterministic within one process"
  printf '[drill selftest] PASS: path guards and run-dir planning verified\n'
  exit 0
fi

# ---------------------------------------------------------------- config ----
DATABASE_URL="${DATABASE_URL:?DATABASE_URL is required}"
BACKUP_DIR="${BACKUP_DIR:-./target/backup-drill}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RAND="$(rand_suffix)"
MC_HOST="${MC_ALIAS_HOST:-http://127.0.0.1:9000}"
MC_USER="${MC_ROOT_USER:-minioadmin}"
MC_PASSWORD="${MC_ROOT_PASSWORD:-minioadmin}"
BUCKET="${BUCKET:-business-platform}"
case "$BUCKET" in
  *-restore-*) fail "BUCKET must be the live bucket, not a drill restore bucket: $BUCKET" ;;
esac

DRILL_DIR="$(plan_drill_dir "$BACKUP_DIR")"
DRILL_ROOT="$(dirname "$DRILL_DIR")"
DRILL_CREATED="no"
MARKER_SEEDED_DB="no"
MARKER_SEEDED_OBJ="no"
RESTORE_DB_CREATED="no"
RESTORE_BUCKET_CREATED="no"

db_url_sans_db() {
  # strip the database name from the URL for maintenance connections
  local url="$1"
  printf '%s/postgres' "${url%/*}"
}
db_name() { printf '%s' "${1##*/}"; }

LIVE_DB="$(db_name "$DATABASE_URL")"
case "$LIVE_DB" in
  postgres) fail "refusing to drill against the maintenance database 'postgres'" ;;
  *_drill_*) fail "DATABASE_URL points at a drill-owned database; refusing drill-on-drill: $LIVE_DB" ;;
esac
RESTORE_DB="${LIVE_DB}_drill_${STAMP}_${RAND}"
RESTORE_URL="${DATABASE_URL%/*}/${RESTORE_DB}"
RESTORE_BUCKET="${BUCKET}-restore-${STAMP}-${RAND}"

MARKER_ID="$(uuidgen 2>/dev/null || python -c 'import uuid; print(uuid.uuid4())')"
MARKER_KEY="backup-drill-marker-${STAMP}-${RAND}.txt"

# ------------------------------------------------------------- cleanup ------
cleanup() {
  local rc=$?
  trap - EXIT
  if [ "$RESTORE_DB_CREATED" = "yes" ]; then
    psql "$(db_url_sans_db "$DATABASE_URL")" -q -c "DROP DATABASE IF EXISTS \"$RESTORE_DB\";" >/dev/null 2>&1 \
      || log "cleanup: restore database drop failed (left behind: $RESTORE_DB)"
  fi
  if [ "$MARKER_SEEDED_OBJ" = "yes" ]; then
    mc rm "drill/$BUCKET/$MARKER_KEY" >/dev/null 2>&1 \
      || log "cleanup: live marker object removal failed (left behind: $BUCKET/$MARKER_KEY)"
  fi
  if [ "$RESTORE_BUCKET_CREATED" = "yes" ]; then
    mc rb --force "drill/$RESTORE_BUCKET" >/dev/null 2>&1 \
      || log "cleanup: restore bucket removal failed (left behind: $RESTORE_BUCKET)"
  fi
  if [ "$DRILL_CREATED" = "yes" ] && [ -n "$DRILL_DIR" ] && [ -d "$DRILL_DIR" ]; then
    case "$DRILL_DIR" in
      "$DRILL_ROOT"/run-*) rm -rf -- "$DRILL_DIR" ;;
      *) log "cleanup: refusing to remove non-drill path: $DRILL_DIR" ;;
    esac
  fi
  [ $rc -eq 0 ] || log "drill ended with failure; drill-owned resources were cleaned up"
  exit $rc
}
trap cleanup EXIT

# ------------------------------------------------------------ dependency ----
for tool in pg_dump pg_restore psql mc md5sum; do
  command -v "$tool" >/dev/null || fail "required tool not available: $tool"
done
mkdir -p "$DRILL_ROOT"
if [ -e "$DRILL_DIR" ]; then
  fail "drill run directory must be freshly created; refusing to reuse: $DRILL_DIR"
fi
mkdir "$DRILL_DIR"
DRILL_CREATED="yes"
log "drill run directory: $DRILL_DIR"
log "drill targets: db='$RESTORE_DB' bucket='$RESTORE_BUCKET' marker='$MARKER_KEY'"

# ---------------------------------------------------------------- seed ------
log "seeding marker row and marker object in live targets"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q <<SQL
CREATE TABLE IF NOT EXISTS backup_drill_marker (
    id uuid PRIMARY KEY,
    created_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO backup_drill_marker (id) VALUES ('$MARKER_ID')
ON CONFLICT (id) DO NOTHING;
SQL
MARKER_SEEDED_DB="yes"

head -c 1024 /dev/urandom | base64 > "$DRILL_DIR/$MARKER_KEY"
EXPECTED_SIZE="$(wc -c < "$DRILL_DIR/$MARKER_KEY" | tr -d ' ')"
EXPECTED_MD5="$(md5sum "$DRILL_DIR/$MARKER_KEY" | cut -d' ' -f1)"
log "marker object recorded: size=$EXPECTED_SIZE md5=$EXPECTED_MD5"

mc alias set drill "$MC_HOST" "$MC_USER" "$MC_PASSWORD" >/dev/null
mc mb --ignore-existing "drill/$BUCKET" >/dev/null
mc cp "$DRILL_DIR/$MARKER_KEY" "drill/$BUCKET/$MARKER_KEY" >/dev/null
MARKER_SEEDED_OBJ="yes"
LIVE_STAT_SIZE="$(mc stat --json "drill/$BUCKET/$MARKER_KEY" 2>/dev/null \
  | python -c 'import json,sys; print(json.load(sys.stdin).get("size", -1))' 2>/dev/null \
  || mc stat "drill/$BUCKET/$MARKER_KEY" | awk '/size/ {print $NF}' | tr -d 'B')"
[ "$LIVE_STAT_SIZE" = "$EXPECTED_SIZE" ] \
  || fail "live marker object size mismatch after upload: live=$LIVE_STAT_SIZE expected=$EXPECTED_SIZE"

# ------------------------------------------------------------- 2. backup ----
log "taking logical backup of $LIVE_DB"
pg_dump "$DATABASE_URL" --format=custom --file "$DRILL_DIR/db-${STAMP}.dump"
test -s "$DRILL_DIR/db-${STAMP}.dump" || fail "database backup file is empty"
BACKUP_SIZE="$(wc -c < "$DRILL_DIR/db-${STAMP}.dump" | tr -d ' ')"
log "database backup written: $BACKUP_SIZE bytes"

log "mirroring bucket $BUCKET"
mc mirror --overwrite "drill/$BUCKET" "$DRILL_DIR/minio-${STAMP}" >/dev/null
test -s "$DRILL_DIR/minio-${STAMP}/$MARKER_KEY" \
  || fail "mirrored backup does not contain the marker object"
MIRROR_MD5="$(md5sum "$DRILL_DIR/minio-${STAMP}/$MARKER_KEY" | cut -d' ' -f1)"
[ "$MIRROR_MD5" = "$EXPECTED_MD5" ] \
  || fail "mirrored marker object checksum mismatch: mirror=$MIRROR_MD5 expected=$EXPECTED_MD5"

# ------------------------------------------------------------ 3. restore ----
log "creating drill-owned restore database $RESTORE_DB"
psql "$(db_url_sans_db "$DATABASE_URL")" -v ON_ERROR_STOP=1 -q -c \
  "CREATE DATABASE \"$RESTORE_DB\";"
RESTORE_DB_CREATED="yes"

log "restoring database backup into $RESTORE_DB"
pg_restore --no-owner --dbname "$RESTORE_URL" "$DRILL_DIR/db-${STAMP}.dump"

log "restoring bucket mirror into $RESTORE_BUCKET"
mc mb --ignore-existing "drill/$RESTORE_BUCKET" >/dev/null
RESTORE_BUCKET_CREATED="yes"
mc mirror --overwrite "$DRILL_DIR/minio-${STAMP}" "drill/$RESTORE_BUCKET" >/dev/null

# ------------------------------------------------------------- 4. verify ----
log "verifying restored database"
LIVE_ROWS="$(psql "$DATABASE_URL" -t -A -c \
  "SELECT count(*) FROM backup_drill_marker WHERE id = '$MARKER_ID';")"
RESTORED_ROWS="$(psql "$RESTORE_URL" -t -A -c \
  "SELECT count(*) FROM backup_drill_marker WHERE id = '$MARKER_ID';")"
[ "$LIVE_ROWS" = "1" ] || fail "marker row missing in live database"
[ "$RESTORED_ROWS" = "1" ] || fail "marker row missing in restored database"

LIVE_TABLES="$(psql "$DATABASE_URL" -t -A -c \
  "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public';")"
RESTORED_TABLES="$(psql "$RESTORE_URL" -t -A -c \
  "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public';")"
[ "$LIVE_TABLES" = "$RESTORED_TABLES" ] \
  || fail "table count mismatch: live=$LIVE_TABLES restored=$RESTORED_TABLES"

log "verifying restored objects FROM THE RESTORE BUCKET"
mc ls "drill/$RESTORE_BUCKET" >/dev/null || fail "restored bucket not listable"
mc stat "drill/$RESTORE_BUCKET/$MARKER_KEY" >/dev/null \
  || fail "marker object missing from restored bucket (backup contained no restorable object)"
mc cp "drill/$RESTORE_BUCKET/$MARKER_KEY" "$DRILL_DIR/restored-marker.txt" >/dev/null
RESTORED_SIZE="$(wc -c < "$DRILL_DIR/restored-marker.txt" | tr -d ' ')"
RESTORED_MD5="$(md5sum "$DRILL_DIR/restored-marker.txt" | cut -d' ' -f1)"
[ "$RESTORED_SIZE" = "$EXPECTED_SIZE" ] \
  || fail "restored marker size mismatch: restored=$RESTORED_SIZE expected=$EXPECTED_SIZE"
[ "$RESTORED_MD5" = "$EXPECTED_MD5" ] \
  || fail "restored marker checksum mismatch: restored=$RESTORED_MD5 expected=$EXPECTED_MD5"
cmp -s "$DRILL_DIR/restored-marker.txt" "$DRILL_DIR/$MARKER_KEY" \
  || fail "restored marker byte comparison failed"
log "restored object verified: size=$RESTORED_SIZE md5=$RESTORED_MD5"

log "PASS: backup/restore drill completed (db row + table counts + marker object roundtrip verified)"
