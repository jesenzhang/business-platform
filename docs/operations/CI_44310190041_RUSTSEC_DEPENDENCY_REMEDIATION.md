# CI 44310190041 `security` 作业 RUSTSEC 依赖修复说明

日期：2026-11-19
分支：`codex/ci-rustsec-dependency-updates`（基于 `main` e3d7c30）
关联 CI：run 44310190041 / security job 134822002420（`cargo audit --deny warnings` 失败：15 vulnerabilities + 4 denied warnings，导致 gitleaks、trivy 未执行）

> 本变更只处理依赖与审计配置；不修改 `.github/workflows/ci.yml`（镜像修复由另一分支负责）。

## 1. Advisory 逐项处置

完整清单以 CI 完整日志与本地 `cargo audit`（rustsec 数据库 1533 条）为准，共 19 条（15 漏洞 + 4 warning）。

### 1.1 通过升级修复（17 条，全部实际验证）

| Advisory | Crate / 原版本 | 修复后版本 | 方式 |
| --- | --- | --- | --- |
| RUSTSEC-2026-0338 / 0340 / 0343 | quinn-proto 0.11.17 | 0.11.19 | Cargo.lock 刷新 |
| RUSTSEC-2026-0523 / 0524 | quinn-proto 0.11.17 | 0.11.19 | Cargo.lock 刷新 |
| RUSTSEC-2026-0538 | quinn-udp 0.5.15 | 0.5.16 | Cargo.lock 刷新 |
| RUSTSEC-2026-0346 / 0347 / 0348 / 0349 | rustls 0.23.45 | 0.23.46 | Cargo.lock 刷新 |
| RUSTSEC-2026-0382 | rustls-webpki 0.103.15 | 0.103.17 | Cargo.lock 刷新 |
| RUSTSEC-2026-0381 | webpki-roots 1.0.9 | 1.0.12 | Cargo.lock 刷新 |
| RUSTSEC-2026-0486 (unsound) | rustix 1.1.4 | 1.1.6 | Cargo.lock 刷新 |
| RUSTSEC-2026-0386 (unsound) | tokio 1.53.1 | 1.54.1 | Cargo.lock 刷新 |
| RUSTSEC-2026-0385 | ctutils 0.4.2 | 0.4.8 | Cargo.lock 刷新 |
| RUSTSEC-2026-0387 | crypto-bigint 0.5.5 | 0.7.11 | 根约束 `p256` 0.13 → 0.14，消除 elliptic-curve 0.13 / crypto-bigint 0.5 链 |
| RUSTSEC-2026-0441 (unmaintained) | pin-utils 0.1.0 | 已移除 | aws-smithy 依赖链刷新后不再解析该 crate |

说明：quinn-* 是 lock 中的可选/非活动项（`cargo tree -i` 无活动消费者），但仍会被 cargo-audit 扫描；刷新 lock 后同样升到 patched 版本。

### 1.2 风险接受（2 条，逐条带理由与复审触发条件，见 `.cargo/audit.toml`）

| Advisory | Crate | 处置与理由 |
| --- | --- | --- |
| RUSTSEC-2026-0459 (CVE-2026-119375) | libsqlite3-sys | **已关闭（2026-11-20，PLAN-0018）**。工作区已迁移至 `sqlx 0.9.1` 与 `libsqlite3-sys 0.40.0`（advisory patched ≥0.39.0），`.cargo/audit.toml` 中对应 ignore 已删除；`cargo audit --deny warnings` 在无该 ignore 的情况下退出 0。不再属于风险接受。 |
| RUSTSEC-2026-0543 (unmaintained) | chrono 0.4.45 | advisory 声明「所有版本受影响、无修复版本」，推荐 jiff/time。chrono 被约 40 个 workspace 成员直接使用（含 public-api-contracts、sqlx chrono feature、各聚合公共 DTO），替换会改变公共时间/序列化契约与 OpenAPI/DB 映射，不属于本变更范围。复审触发：发布 patched 版本 / 专门迁移计划落地 / 新增针对锁定版本的安全公告。 |
| RUSTSEC-2023-0071 | rsa 0.9.10 | 仅经 sqlx 可选 `mysql` feature 进入 lock，工作区只启用 postgres+sqlite，从未编译。rsa 0.9.x 无修复；上游 sqlx 0.8 不接受 rsa ≥0.10。复审触发：sqlx 接受修复版本 / 移除依赖 / 启用 mysql 并实际编译 rsa。 |

## 2. Cargo.lock 变更摘要

- 刷新 150+ 个包到各自 semver 兼容的最新版本；安全相关关键项见第 1.1 节。
- `p256` 0.13.2 → 0.14.0，并连带 `ecdsa` 0.16.9 → 0.17.0、`elliptic-curve` 0.13.8 → 0.14.3、`crypto-bigint` 0.5.5 → 0.7.11。
- `pin-utils` 0.1.0 从 lock 中移除；`libsqlite3-sys` 在 PLAN-0018 中进一步提升至 0.40.0。
- AWS SDK 全家桶（aws-sdk-s3 1.144→1.161 等）、tokio 1.53.1→1.54.1、rustls 栈、hyper/h2、pest、wasm-bindgen 等常规刷新。

## 3. 源码兼容改动（最小）

p256 0.14（ecdsa 0.17）将 `VerifyingKey::to_encoded_point` 改名为 `to_sec1_point`。仅在两处**测试辅助函数**做机械改名，无业务逻辑变化：

- `apps/business-api/tests/harness/mod.rs:71`
- `apps/business-api/tests/oidc_auth.rs:157`

## 4. 本地门禁结果

| 门禁 | 结果 | 备注 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS (EXIT=0) | |
| `cargo check --workspace --all-targets --all-features` | PASS (EXIT=0) | 因本机 MSVC 链接器/rustc OOM（LNK1102 / STATUS_STACK_BUFFER_OVERRUN），使用 `CARGO_BUILD_JOBS=1`、`CARGO_PROFILE_DEV_DEBUG=0` 完成 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS (EXIT=0) | 同上单 job 设置 |
| `cargo audit --deny warnings` | PASS (EXIT=0) | 15 vulnerabilities + 4 warnings 全部清零（3 项为显式 documented ignore） |
| `cargo test --workspace --all-features` | 已启动，提交时仍在编译测试二进制（本机内存受限，单 job 执行） | 未计入「通过」；最终结果在交付报告中如实说明 |

未执行：`.github/workflows/ci.yml` 的 gitleaks / trivy / 镜像修复（属于其他分支与 CI 环境）。

## 5. 回滚

本变更可独立回滚。示例：

```bash
git revert <本提交SHA>      # 推荐：生成反向提交
# 或（未 push 时）
git reset --hard e3d7c30
```

回滚后恢复原有 15 vulnerabilities + 4 denied warnings 的 CI 失败状态；无任何数据库/配置/运行时副作用。
