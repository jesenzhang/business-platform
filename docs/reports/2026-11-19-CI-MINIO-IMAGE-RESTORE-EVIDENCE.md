# CI MinIO image restore — registry evidence (2026-11-19)

Scope: all active MinIO image references (`.github/workflows/ci.yml`,
`scripts/test-postgres-minio-multiprocess.sh`, `deploy/docker-compose.yml`,
`deploy/dev/compose.yml`, `deploy/demo/docker-compose.yml`). Branch
`codex/ci-minio-image-restore` (base `e3d7c30`).
Goal: make the `integration` job pull real MinIO server + client images again without
changing Rust code, Cargo manifests, healthchecks, bucket init, contract tests or E2E.

## 1. Root cause reproduced against the registries

| Registry / URL | Observed fact |
| --- | --- |
| `https://quay.io/v2/auth?service=quay.io&scope=repository:minio/minio:pull` | HTTP 200; anonymous JWT payload: `{"sub":"(anonymous)","access":[{"name":"minio/minio","actions":[]}],"context":{"com.apostille.roots":{"minio/minio":"$disabled"}}}` |
| `https://quay.io/v2/minio/minio/manifests/RELEASE.2024-06-13T22-53-53Z` (Bearer anon token) | HTTP 401 `{"code":"UNAUTHORIZED","message":"access to the requested resource is not authorized"}` — identical to CI run 44310190041 |
| `https://quay.io/v2/auth?service=quay.io&scope=repository:minio/mc:pull` | Same: `actions: []`, `com.apostille.roots: "$disabled"` |
| `https://quay.io/v2/minio/mc/manifests/RELEASE.2024-06-12T14-34-03Z` | HTTP 401 same message |
| `https://hub.docker.com/v2/repositories/minio/minio/tags/?page_size=10` | HTTP 404 `object not found` |
| `https://hub.docker.com/v2/repositories/minio/mc/tags/?page_size=10` | HTTP 404 `object not found` |
| `https://ghcr.io/token?service=ghcr.io&scope=repository:minio/minio:pull` | HTTP 403 `DENIED` (no public package) |

Upstream confirms image publication stopped: maintainer comment in
`https://api.github.com/repos/minio/minio/issues/21647/comments` (comment 3418675115):
> "This project is a source only distribution now, if you want to build containers you need to build them yourselves"

Therefore no official public MinIO image can be restored; a public mirror of the
**real upstream MinIO binaries** is required (not RustFS/Garage/S3-compatible rewrites).

## 2. Candidate evaluation (all checked via registry API, not guesswork)

| Candidate | Anonymous manifest | Fixed tag | Upstream layout | curl | mc | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| `openebs/minio:RELEASE.2024-12-18T13-15-44Z` | 200, digest `sha256:cf23643a6cf9ce159c57643ceb88279e431262282428c9e0bf3a7ef1a97e84b4` | yes (only tag) | `ENTRYPOINT /usr/bin/docker-entrypoint.sh`, `CMD minio`, official env vars | yes `/usr/bin/curl` | yes `/usr/bin/mc` | **chosen** |
| `coollabsio/minio:RELEASE.2025-10-15T17-29-55Z` | 200 | yes | built from official source (`https://api.github.com/repos/coollabsio/minio/contents/Dockerfile`) | **no** (all 8 amd64 layers scanned: no curl/wget) | yes | rejected: would break existing `curl` healthcheck |
| `bitnamilegacy/minio:2025.7.23-debian-12-r5` | 200 | yes | custom Bitnami entrypoint, `USER 1001`, `/bitnami/minio/data` root-owned 0775 | yes | yes | rejected: custom entrypoint/data-dir semantics risk startup failure |
| `bitnamilegacy/minio-client:2025.7.21-debian-12-r3` | 200 | yes | mc on PATH | — | yes | viable fallback client only |
| `alpine/minio:RELEASE.2025-10-15T17-29-55Z` | 200 | yes | `ENTRYPOINT minio`, `USER minio` | no | no | rejected |
| `ghcr.io/minio/minio`, `ghcr.io/minio/mc` | 403 | — | — | — | — | unavailable |

Docker Hub API facts:
- `https://hub.docker.com/v2/repositories/openebs/minio/tags/?page_size=15` → HTTP 200, `count=1`, tag `RELEASE.2024-12-18T13-15-44Z`.
- `https://hub.docker.com/v2/repositories/bitnamilegacy/minio/tags/?page_size=5` → HTTP 200, `count=1027`, fixed tags present.
- `https://hub.docker.com/v2/repositories/coollabsio/minio/tags/?page_size=10` → HTTP 200, fixed `RELEASE.*` tags present.

`openebs/minio` image was verified by pulling the anonymous registry token from
`https://auth.docker.io/token?service=registry.docker.io&scope=repository:openebs/minio:pull`,
reading the manifest and config blob from `registry-1.docker.io/v2/openebs/minio/...`,
and enumerating every amd64 layer's tar entries. The layer set contains
`usr/bin/minio`, `usr/bin/mc`, `usr/bin/curl`, `usr/bin/docker-entrypoint.sh`,
`usr/bin/sh`, `usr/bin/bash`, and the config has no custom `User` (upstream-compatible).

The existing pinned binary mc install is unaffected:
`https://github.com/minio/mc/releases/download/RELEASE.2025-08-13T08-35-41Z/mc.linux-amd64.RELEASE.2025-08-13T08-35-41Z`
→ HTTP 200 (30,535,864 bytes), and the matching `.sha256sum` release asset exists.
Its checksum step in `ci.yml` is unchanged.

## 3. Change made

- Service image: `quay.io/minio/minio:RELEASE.2024-06-13T22-53-53Z` → `openebs/minio:RELEASE.2024-12-18T13-15-44Z`.
- Bucket-init client image: `quay.io/minio/mc:RELEASE.2024-06-12T14-34-03Z` → same pinned `openebs/minio` tag,
  with `--entrypoint /usr/bin/sh` (the image has `/usr/bin/sh`, not a literal `/bin/sh`).
- Pin rationale comments updated with the registry facts above.
- Healthcheck (`curl -f http://localhost:9000/minio/health/live`), root credentials, ports,
  `command: server /data`, bucket init, pinned mc binary install, contract tests and E2E steps: untouched.
- No Cargo.toml / Cargo.lock / Rust changes.

## 4. Rollback

Image-reference-only change: revert the image commit(s) on this branch. No Cargo,
Rust source, migration or API change is included.

## 5. NOT RUN / residual risk

- **NOT RUN**: `docker pull` / container startup. No Docker daemon in this workspace.
  Pullability was verified through the Docker Registry HTTP API (anonymous token + manifest 200),
  and image contents through config/layer inspection; this is not a runtime proof.
- **NOT RUN**: real PostgreSQL + MinIO contract tests, E2E and backup/restore drill (CI-only).
- **Risk**: `openebs/minio` is a community mirror ("not maintained by OpenEBS"), not an
  upstream-published image. Its single available tag is an older upstream release
  (2024-12-18) than the current pinned mc binary (2025-08-13); the mc/CI contract surface used
  here (`alias set`, `mb --ignore-existing`, `anonymous set none`, `/minio/health/live`) is
  supported by both. If the mirror is deleted, pin to a digest or build from upstream source.
- **Follow-up in the same change**: CI validation run `44474318859` proved the
  service image pulls and all real PostgreSQL + MinIO contract suites pass, but the
  multiprocess E2E script still pulled the retired `quay.io/minio/mc` tag and
  failed with the same 401. The script's two client `docker run` calls and all
  three compose files were moved to the same pinned image; compose healthchecks
  using `mc ready local` were switched to the verified
  `curl -f http://localhost:9000/minio/health/ready` endpoint. These are config
  edits only; compose stack startup is NOT RUN locally.
