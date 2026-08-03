#!/usr/bin/env bash
set -Eeuo pipefail

: "${DATABASE_URL:=postgres://postgres:postgres@127.0.0.1:5432/business_platform}"
: "${MINIO_ENDPOINT:=http://127.0.0.1:9000}"
: "${MINIO_ACCESS_KEY:=minioadmin}"
: "${MINIO_SECRET_KEY:=minioadmin}"
: "${MINIO_BUCKET:=contract-test-bucket}"

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/business-platform-pg-e2e.XXXXXX")"
log_dir="$work/logs"
mkdir -p "$log_dir"
api_pid=""
business_pid=""
ai_pid=""
port="${BUSINESS_E2E_PORT:-18080}"
tenant="$(cat /proc/sys/kernel/random/uuid)"
user="$(cat /proc/sys/kernel/random/uuid)"

cleanup() {
  set +e
  for pid in "$ai_pid" "$business_pid" "$api_pid"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
  done
  wait "$ai_pid" "$business_pid" "$api_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

cd "$repo"
cargo build --quiet -p business-api -p business-worker -p ai-worker

printf 'MinIO process E2E source\n' > "$work/source.txt"
for _ in $(seq 1 250000); do
  printf 'durable recovery padding\n' >> "$work/source.txt"
done
source_size="$(wc -c < "$work/source.txt" | tr -d ' ')"
key="tenants/$tenant/documents/source/v1/source.txt"
docker run --network host --rm -v "$work:/work:ro" --entrypoint /bin/sh \
  minio/mc:RELEASE.2024-06-12T14-34-03Z -c \
  "mc alias set local '$MINIO_ENDPOINT' '$MINIO_ACCESS_KEY' '$MINIO_SECRET_KEY' >/dev/null && mc mb --ignore-existing local/$MINIO_BUCKET >/dev/null && mc cp /work/source.txt local/$MINIO_BUCKET/$key >/dev/null"

export BUSINESS_API__ENV=development
export BUSINESS_API__SERVER__HOST=127.0.0.1
export BUSINESS_API__SERVER__PORT="$port"
export BUSINESS_API__DATABASE__BACKEND=postgres
export BUSINESS_API__DATABASE__URL="$DATABASE_URL"
export BUSINESS_API__AUTH__DEV_AUTH_ENABLED=true
export BUSINESS_API__AUTH__DEV_SECRET=local-pg-e2e-only

export BUSINESS_WORKER__ENV=development
export BUSINESS_WORKER__DATABASE__BACKEND=postgres
export BUSINESS_WORKER__DATABASE__URL="$DATABASE_URL"
export BUSINESS_WORKER__STORAGE__BACKEND=s3
export BUSINESS_WORKER__STORAGE__ENDPOINT="$MINIO_ENDPOINT"
export BUSINESS_WORKER__STORAGE__BUCKET="$MINIO_BUCKET"
export BUSINESS_WORKER__STORAGE__ACCESS_KEY="$MINIO_ACCESS_KEY"
export BUSINESS_WORKER__STORAGE__SECRET_KEY="$MINIO_SECRET_KEY"
export BUSINESS_WORKER__AI_MODE=separate
export BUSINESS_WORKER__CONCURRENCY=4
export BUSINESS_WORKER__WORKER_ID=business-e2e
export BUSINESS_WORKER__TEST_STEP_DELAY_MILLIS=1000

export AI_WORKER__ENV=development
export AI_WORKER__DATABASE__BACKEND=postgres
export AI_WORKER__DATABASE__URL="$DATABASE_URL"
export AI_WORKER__STORAGE__BACKEND=s3
export AI_WORKER__STORAGE__ENDPOINT="$MINIO_ENDPOINT"
export AI_WORKER__STORAGE__BUCKET="$MINIO_BUCKET"
export AI_WORKER__STORAGE__ACCESS_KEY="$MINIO_ACCESS_KEY"
export AI_WORKER__STORAGE__SECRET_KEY="$MINIO_SECRET_KEY"
export AI_WORKER__WORKER_ID=ai-e2e
export AI_WORKER__CONCURRENCY=2
export AI_WORKER__TEST_TASK_DELAY_MILLIS=1000

"$repo/target/debug/business-api" > "$log_dir/api.log" 2>&1 & api_pid=$!
"$repo/target/debug/business-worker" > "$log_dir/business-worker.log" 2>&1 & business_pid=$!
"$repo/target/debug/ai-worker" > "$log_dir/ai-worker.log" 2>&1 & ai_pid=$!

base="http://127.0.0.1:$port"
for _ in $(seq 1 120); do
  if curl -fsS "$base/health/ready" >/dev/null 2>&1; then break; fi
  sleep 0.5
done
curl -fsS "$base/health/ready" >/dev/null

auth=(-H "Authorization: Bearer local-pg-e2e-only" -H "X-Tenant-Id: $tenant" -H "X-User-Id: $user")
doc_key="document-$RANDOM-$(date +%s)"
doc_json="$(curl -fsS -X POST "$base/api/v1/documents" "${auth[@]}" -H "Idempotency-Key: $doc_key" -H 'Content-Type: application/json' -d "{\"original_filename\":\"source.txt\",\"content_type\":\"text/plain\",\"object_key\":\"$key\",\"size_bytes\":$source_size}")"
document_id="$(jq -r '.data.id' <<<"$doc_json")"
[[ "$document_id" != "null" && -n "$document_id" ]]
job_json="$(curl -fsS -X POST "$base/api/v1/documents/$document_id/processing-jobs" "${auth[@]}" -H "Idempotency-Key: job-$RANDOM-$(date +%s)" -H 'Content-Type: application/json' -d '{"content_revision":1}')"
job_id="$(jq -r '.data.job_id' <<<"$job_json")"
[[ "$job_id" != "null" && -n "$job_id" ]]

# Kill the business worker while its first leased step is running, then let a
# fresh process reclaim the expired lease and resume from current_step.
business_crash=0
for _ in $(seq 1 120); do
  status_json="$(curl -fsS "$base/api/v1/processing-jobs/$job_id" "${auth[@]}")"
  status="$(jq -r '.data.status' <<<"$status_json")"
  if [[ "$status" == "running" ]]; then
    kill -9 "$business_pid" 2>/dev/null || true
    set +e
    wait "$business_pid"
    set -e
    "$repo/target/debug/business-worker" >> "$log_dir/business-worker.log" 2>&1 & business_pid=$!
    business_crash=1
    break
  fi
  sleep 0.1
done
[[ "$business_crash" == 1 ]]

# Wait until the AI task has its own lease, kill that process, and verify AI
# reclaim resumes without creating a second task or candidate.
ai_crash=0
for _ in $(seq 1 600); do
  status_json="$(curl -fsS "$base/api/v1/processing-jobs/$job_id" "${auth[@]}")"
  status="$(jq -r '.data.status' <<<"$status_json")"
  if [[ "$status" == "waiting_for_ai" ]]; then
    task_status="$(psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -Atc "SELECT status FROM document_ai_tasks WHERE tenant_id = '$tenant' AND job_id = '$job_id' ORDER BY created_at DESC LIMIT 1" | tr -d '[:space:]')"
    if [[ "$task_status" == "running" ]]; then
      kill -9 "$ai_pid" 2>/dev/null || true
      set +e
      wait "$ai_pid"
      set -e
      "$repo/target/debug/ai-worker" >> "$log_dir/ai-worker.log" 2>&1 & ai_pid=$!
      ai_crash=1
      break
    fi
  fi
  sleep 0.1
done
[[ "$ai_crash" == 1 ]]

status=""
for _ in $(seq 1 120); do
  status_json="$(curl -fsS "$base/api/v1/processing-jobs/$job_id" "${auth[@]}")"
  status="$(jq -r '.data.status' <<<"$status_json")"
  case "$status" in
    waiting_for_review) break ;;
    failed|cancelled|rejected) echo "$status_json"; cat "$log_dir"/*.log; exit 1 ;;
  esac
  sleep 0.5
done
[[ "$status" == waiting_for_review ]]

candidate_json="$(curl -fsS "$base/api/v1/processing-jobs/$job_id/candidate" "${auth[@]}")"
candidate_version="$(jq -r '.data.version' <<<"$candidate_json")"
review_key="review-$RANDOM-$(date +%s)"
review_body="{\"decision\":\"accepted\",\"candidate_version\":$candidate_version}"
review_json="$(curl -fsS -X POST "$base/api/v1/processing-jobs/$job_id/review" "${auth[@]}" -H "Idempotency-Key: $review_key" -H 'Content-Type: application/json' -d "$review_body")"
[[ "$(jq -r '.data.candidate_id' <<<"$review_json")" != "null" ]]
# A lost HTTP response must be replayable after the job is terminal.
replay_json="$(curl -fsS -X POST "$base/api/v1/processing-jobs/$job_id/review" "${auth[@]}" -H "Idempotency-Key: $review_key" -H 'Content-Type: application/json' -d "$review_body")"
[[ "$(jq -r '.data.candidate_id' <<<"$replay_json")" != "null" ]]
final_json="$(curl -fsS "$base/api/v1/processing-jobs/$job_id" "${auth[@]}")"
[[ "$(jq -r '.data.status' <<<"$final_json")" == succeeded ]]

job_ids=()
for _ in $(seq 1 20); do
  key_suffix="job-$RANDOM-$(date +%s%N)"
  queued_json="$(curl -fsS -X POST "$base/api/v1/documents/$document_id/processing-jobs" "${auth[@]}" -H "Idempotency-Key: $key_suffix" -H 'Content-Type: application/json' -d '{"content_revision":1}')"
  queued_id="$(jq -r '.data.job_id' <<<"$queued_json")"
  [[ "$queued_id" != "null" && -n "$queued_id" ]]
  job_ids+=("$queued_id")
done
for queued_id in "${job_ids[@]}"; do
  queued_status=""
  for _ in $(seq 1 180); do
    queued_status="$(curl -fsS "$base/api/v1/processing-jobs/$queued_id" "${auth[@]}" | jq -r '.data.status')"
    case "$queued_status" in
      waiting_for_review) break ;;
      failed|cancelled|rejected) exit 1 ;;
    esac
    sleep 0.5
  done
  [[ "$queued_status" == waiting_for_review ]]
  queued_candidate="$(curl -fsS "$base/api/v1/processing-jobs/$queued_id/candidate" "${auth[@]}")"
  queued_version="$(jq -r '.data.version' <<<"$queued_candidate")"
  curl -fsS -X POST "$base/api/v1/processing-jobs/$queued_id/review" "${auth[@]}" -H "Idempotency-Key: review-$queued_id" -H 'Content-Type: application/json' -d "{\"decision\":\"accepted\",\"candidate_version\":$queued_version}" >/dev/null
  [[ "$(curl -fsS "$base/api/v1/processing-jobs/$queued_id" "${auth[@]}" | jq -r '.data.status')" == succeeded ]]
done

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -Atc "SELECT COUNT(*) FROM document_extraction_candidates WHERE tenant_id = '$tenant' AND job_id = '$job_id'" | grep -qx '1'
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -Atc "SELECT COUNT(*) FROM document_extraction_reviews WHERE tenant_id = '$tenant' AND candidate_id IN (SELECT id FROM document_extraction_candidates WHERE tenant_id = '$tenant' AND job_id = '$job_id')" | grep -qx '1'
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -Atc "SELECT COUNT(*) FROM document_ai_tasks WHERE tenant_id = '$tenant' AND job_id = '$job_id' AND status = 'succeeded'" | grep -qx '1'

echo "PostgreSQL + MinIO multi-process E2E: PASS"
echo "Separate AI review replay and 20-job concurrency: PASS"
echo "Business Worker crash recovery: PASS"
echo "AI Worker crash recovery: PASS"
