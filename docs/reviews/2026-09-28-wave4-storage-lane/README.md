# PLAN-0014 Wave 4 — real S3-compatible provider lane

> **Read together with Round 2.** The cleanup and bucket-teardown cases described
> below no longer exist: Round 2 found that `lane_leaves_no_objects_under_the_run_prefix`
> listed the prefix once (one page of up to 1000 keys) and never inspected
> incomplete multipart uploads, so its "prefix empty" verdict could not be
> trusted, and the teardown case could delete CI's shared `contract-test-bucket`.
> The records below are kept verbatim as what Round 1 observed; the corrected
> split (contract step · fault scenarios · final cleanup) and its re-execution are
> in [`../2026-09-28-wave4-round2-storage-lane/`](../2026-09-28-wave4-round2-storage-lane/).

## Provider actually used (and why)

| Item | Value |
|---|---|
| Implementation | RustFS `1.0.1-preview.11` (S3-compatible object store) |
| Release tag commit | `0913686b12845eefc69a2f4ed4ee6a8161e25742` |
| Archive SHA-256 (verified after download) | `cae2628159a8759118d8a68cd043c389a05ee9a6a13ceb9e188e9414578453d5` |
| Invocation | `rustfs.exe server <volume> --address 127.0.0.1:19100 --access-key <dedicated> --secret-key <dedicated>` (console left off; `--console-enable` takes no value) |
| Health probe | `GET /minio/health/live` → 200 |
| Dedicated bucket / prefix | `plan0014-wave4-bucket`, `plan0014-wave4/<WAVE4_S3_RUN_ID>/` |
| Credentials | dedicated throwaway test identity, never committed |

MinIO was the first choice and is **unavailable**: `dl.min.io/server/minio/release/windows-amd64/minio.exe`
returns HTTP 410 Gone ("no longer served") and the final open-source release
tag publishes no Windows asset. Real AWS S3 was not used: no cloud credentials
are in scope for this repository. Both facts are recorded instead of being
papered over; `crates/object-storage/tests/minio_contract.rs` therefore stays
`#[ignore]`d and unexecuted (see gap `ST-W4-06`).

## Reproduction

```powershell
$env:WAVE4_S3_ENDPOINT = 'http://127.0.0.1:19100'
$env:WAVE4_S3_ACCESS_KEY = '<dedicated>'      # not committed
$env:WAVE4_S3_SECRET_KEY = '<dedicated>'      # not committed
$env:WAVE4_S3_BUCKET = 'plan0014-wave4-bucket'
$env:WAVE4_S3_REGION = 'us-east-1'
$env:WAVE4_S3_RUN_ID = 'run-20260928-110319'
cargo test -p object-storage --test s3_contract_lane --all-features -- --ignored --test-threads=1 --nocapture
# leak proof: second invocation, same RUN_ID, cleanup case only
cargo test -p object-storage --test s3_contract_lane --all-features -- --ignored --test-threads=1 --nocapture lane_leaves_no_objects_under_the_run_prefix
# teardown: dedicated bucket only
$env:WAVE4_S3_DELETE_BUCKET = 'true'
cargo test -p object-storage --test s3_contract_lane --all-features -- --ignored --test-threads=1 --nocapture lane_deletes_the_dedicated_bucket_when_requested
```

## Result

| Log | Content | Result |
|---|---|---|
| — (not retained) | first execution (`run-20260928-103640`) ran before this evidence directory existed; its stdout was not kept, so only the outcome is recorded here and in the review test matrix | 7 passed / 0 failed, but two capability probes were later identified as probe bugs: a 16-byte part below the S3 5 MiB minimum, and a hand-written CRC32 digest that is not valid Base64 |
| `02-s3-contract-lane-corrected.log` | corrected probes; the multipart case asserted provider behaviour | 6 passed / **1 failed** — the failure was the test demanding that the provider complete a multipart upload |
| `03-s3-contract-lane-final.log` | capability probes record instead of dictating; same RUN_ID reused for the cleanup pass | 7 passed / 0 failed; cleanup pass 1 passed; bucket teardown 1 passed |
| `04-teardown.md` | process stop + resource removal | health probe refused; 0 objects under the prefix; bucket deleted |

## Capability observations for this provider (not generalised)

| Capability | Observation |
|---|---|
| `PutObject` / `HeadObject` / `GetObject` / `DeleteObject` round trip | supported; user metadata and payload round-trip; delete of an absent object is idempotent |
| Presigned GET | key-scoped and time-bounded: valid URL 200, tampered path 403/404, 1 s grant after 3 s 403/404 |
| Conditional read `If-Match` | honoured (`conditional_read if_match: true`), stale ETag rejected (`stale_etag_rejected: true`) |
| Object checksums | `PutObject` with `ChecksumAlgorithm=Crc32` accepted, checksum echoed on write and returned by `HeadObject` (`write_echoed=Some("xjIjIw==")`) |
| Multipart upload | `CreateMultipartUpload` and `AbortMultipartUpload` succeed; a legal 5 MiB `UploadPart` is answered with `HTTP 500 InternalError` (`aws_request_id c80813ec-37f3-4902-aff3-31bd027d66b8`), so `CompleteMultipartUpload` cannot run. Recorded as `explicit_outcome: true` because the failure is an explicit provider error, not a silent partial success |
| Bucket versioning | `GetBucketVersioning` returns no status (`unconfigured`) |

None of these rows can affect this repository today: the
`ObjectStorageClient` port exposes no multipart, no checksum parameter and no
versioning concept (gaps `ST-W4-01`, `ST-W4-07`).
