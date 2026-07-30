# 企业 AI 业务平台基础设施开发、验证与预生产方案

> 版本：v1.0  
> 状态：正式建议稿  
> 日期：2026-07-30  
> 关联文档：《企业 AI 业务平台与智能助手总体架构方案_v2.md》  
> 适用范围：Rust 企业业务平台在尚未具备正式 PostgreSQL、MinIO/S3、消息系统及可观测性平台时的开发、测试、验证与预生产建设

---

## 1. 文档目的

本文解决以下问题：

- 当前尚未部署正式 PostgreSQL。
- 当前尚未部署正式 MinIO 或其他 S3 兼容对象存储。
- 消息系统、监控、备份恢复和预生产环境尚未完整建设。
- Rust 后端开发不能等待正式基础设施完成后再开始。
- 本地开发和自动化测试不能长期依赖不稳定的共享服务。
- 进入生产前必须证明数据库、对象存储、消息和任务系统可以部署、恢复和运维。

本方案采用分层验证体系：

```text
纯单元测试
    ↓
本地 Docker Compose
    ↓
Testcontainers 集成测试
    ↓
CI 端到端测试
    ↓
持久化预生产环境
    ↓
正式生产环境
```

核心原则：

> 正式基础设施尚未就绪时，使用本地容器和自动化临时环境开发；上线前通过持久化预生产环境完成备份、恢复、故障和性能验证。

---

## 2. 总体原则

### 2.1 开发不依赖正式环境

开发人员应能够在本机一键启动开发所需依赖，不依赖公共测试数据库、共享 MinIO、运维临时账号或长期运行的远程测试服务器。

### 2.2 Fake 只用于单元测试

Fake Repository、MemoryObjectStorage、InMemoryMessageBus 和 Mock AI Provider 只用于快速验证业务规则。

涉及以下能力时，必须使用真实基础设施测试：

- SQL、事务和锁
- 数据库约束和索引
- S3 bucket、object key 和 metadata
- 消息重复投递、Ack 和重放
- 服务断开、超时和恢复
- 文件流式上传和下载

### 2.3 不使用 SQLite 替代 PostgreSQL 集成验证

SQLite 与 PostgreSQL 在 SQL、JSONB、事务隔离、锁、并发、时间类型、索引和约束行为上存在差异。SQLite 可以用于独立小工具，但不能作为 PostgreSQL 业务系统的最终集成测试替代品。

### 2.4 不使用本地文件系统替代 S3 集成验证

本地文件系统无法验证 bucket、object key、Content-Type、metadata、预签名 URL、权限、网络超时、分片上传和对象存储错误语义。

### 2.5 业务侧只依赖 S3 接口

开发阶段使用 MinIO，不代表生产必须使用 MinIO。生产可选择 MinIO、其他 S3 兼容存储或公有云 S3。业务代码不得依赖 MinIO 专有能力。

---

## 3. 环境分层

| 环境 | PostgreSQL | 对象存储 | 消息系统 | 用途 |
|---|---|---|---|---|
| 单元测试 | Fake Repository | MemoryObjectStorage | InMemoryMessageBus | 业务规则快速验证 |
| 本地开发 | Docker PostgreSQL | Docker MinIO | Docker NATS，可选 | 日常开发和调试 |
| 集成测试 | Testcontainers PostgreSQL | Testcontainers MinIO | Testcontainers NATS | 真实依赖自动验证 |
| CI 端到端 | Docker Compose | Docker Compose | Docker Compose | 完整流程验证 |
| 预生产 | 独立持久化 PostgreSQL | 独立 MinIO/S3 | 独立消息系统 | 备份、恢复、压测、升级和故障演练 |
| 生产 | 高可用 PostgreSQL | 正式对象存储 | 正式消息系统 | 生产运行 |

---

## 4. 推荐仓库结构

```text
enterprise-platform/
├── Cargo.toml
├── apps/
│   ├── business-api/
│   ├── business-worker/
│   ├── ai-worker/
│   ├── agent-adapter/
│   └── migration/
├── crates/
│   ├── object-storage/
│   ├── messaging/
│   ├── observability/
│   ├── test-support/
│   └── ...
├── migrations/
├── config/
│   ├── development.toml
│   ├── test.toml
│   ├── staging.toml
│   └── production.toml.example
├── deploy/
│   ├── dev/
│   │   ├── compose.yml
│   │   ├── .env.example
│   │   └── README.md
│   ├── ci/
│   │   └── compose.yml
│   └── staging/
│       ├── compose.yml
│       ├── env.example
│       └── README.md
├── scripts/
│   ├── dev-up.ps1
│   ├── dev-down.ps1
│   ├── dev-reset.ps1
│   ├── dev-check.ps1
│   ├── migrate.ps1
│   ├── test-integration.ps1
│   ├── test-e2e.ps1
│   ├── backup-staging.ps1
│   └── restore-staging.ps1
└── tests/
    ├── integration/
    ├── contract/
    ├── e2e/
    └── fixtures/
```

---

## 5. 本地开发基础设施

首期最低组合：

```text
PostgreSQL + MinIO
```

推荐完整组合：

```text
PostgreSQL
+ MinIO
+ NATS JetStream
+ Mock AI Server
```

可选增加：

- OpenTelemetry Collector
- Prometheus
- Grafana
- Mailpit
- Redis

---

## 6. Docker Compose 方案

文件：

```text
deploy/dev/compose.yml
```

```yaml
name: enterprise-platform-dev

services:
  postgres:
    image: postgres:18.4-bookworm
    container_name: enterprise-platform-postgres
    environment:
      POSTGRES_DB: enterprise_platform
      POSTGRES_USER: enterprise
      POSTGRES_PASSWORD: enterprise_dev_only
      POSTGRES_INITDB_ARGS: "--auth-host=scram-sha-256"
    ports:
      - "127.0.0.1:5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U enterprise -d enterprise_platform"]
      interval: 5s
      timeout: 5s
      retries: 20
      start_period: 10s
    restart: unless-stopped

  minio:
    image: minio/minio:latest
    container_name: enterprise-platform-minio
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: enterprise_minio
      MINIO_ROOT_PASSWORD: enterprise_minio_dev_only
    ports:
      - "127.0.0.1:9000:9000"
      - "127.0.0.1:9001:9001"
    volumes:
      - minio_data:/data
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:9000/minio/health/live"]
      interval: 5s
      timeout: 5s
      retries: 20
      start_period: 10s
    restart: unless-stopped

  minio-init:
    image: minio/mc:latest
    container_name: enterprise-platform-minio-init
    depends_on:
      minio:
        condition: service_healthy
    entrypoint:
      - /bin/sh
      - -c
      - |
        mc alias set local http://minio:9000 enterprise_minio enterprise_minio_dev_only
        mc mb --ignore-existing local/enterprise-documents
        mc mb --ignore-existing local/enterprise-temp
        mc mb --ignore-existing local/enterprise-exports
        mc mb --ignore-existing local/enterprise-backups
        echo "MinIO buckets initialized."

  nats:
    image: nats:2.11-alpine
    container_name: enterprise-platform-nats
    command: ["-js", "-m", "8222", "--store_dir", "/data"]
    ports:
      - "127.0.0.1:4222:4222"
      - "127.0.0.1:8222:8222"
    volumes:
      - nats_data:/data
    healthcheck:
      test: ["CMD-SHELL", "wget -qO- http://localhost:8222/healthz | grep -q ok"]
      interval: 5s
      timeout: 5s
      retries: 20
      start_period: 10s
    restart: unless-stopped

volumes:
  postgres_data:
  minio_data:
  nats_data:
```

### 6.1 镜像版本规则

PoC 阶段可以暂时使用 `latest`。进入正式项目后，CI、预生产和生产必须固定版本或 digest。

禁止使用浮动镜像完成可重复构建和发布。

---

## 7. 本地配置

文件：

```text
config/development.toml
```

```toml
[server]
host = "127.0.0.1"
port = 8080

[database]
url = "postgres://enterprise:enterprise_dev_only@127.0.0.1:5432/enterprise_platform"
max_connections = 20
min_connections = 2
connect_timeout_seconds = 5
idle_timeout_seconds = 300

[object_storage]
provider = "s3"
endpoint = "http://127.0.0.1:9000"
region = "us-east-1"
access_key = "enterprise_minio"
secret_key = "enterprise_minio_dev_only"
force_path_style = true
bucket_documents = "enterprise-documents"
bucket_temp = "enterprise-temp"
bucket_exports = "enterprise-exports"
bucket_backups = "enterprise-backups"

[messaging]
provider = "nats"
url = "nats://127.0.0.1:4222"
stream = "ENTERPRISE_PLATFORM"

[observability]
log_level = "debug"
log_format = "pretty"
otel_enabled = false
```

正式环境不得把真实密钥写入 Git 中的配置文件。

---

## 8. 本地一键脚本

### 8.1 `scripts/dev-up.ps1`

```powershell
$ErrorActionPreference = "Stop"

$ComposeFile = Join-Path $PSScriptRoot "../deploy/dev/compose.yml"

docker compose -f $ComposeFile up -d
docker compose -f $ComposeFile ps

Write-Host "PostgreSQL: postgres://enterprise:enterprise_dev_only@127.0.0.1:5432/enterprise_platform"
Write-Host "MinIO API:  http://127.0.0.1:9000"
Write-Host "MinIO UI:   http://127.0.0.1:9001"
Write-Host "NATS:       nats://127.0.0.1:4222"
Write-Host "NATS UI:    http://127.0.0.1:8222"
```

### 8.2 `scripts/dev-down.ps1`

```powershell
$ErrorActionPreference = "Stop"

$ComposeFile = Join-Path $PSScriptRoot "../deploy/dev/compose.yml"
docker compose -f $ComposeFile down
```

该命令保留数据卷。

### 8.3 `scripts/dev-reset.ps1`

```powershell
$ErrorActionPreference = "Stop"

$ComposeFile = Join-Path $PSScriptRoot "../deploy/dev/compose.yml"

docker compose -f $ComposeFile down --volumes --remove-orphans
docker compose -f $ComposeFile up -d
```

该命令删除全部本地开发数据。

### 8.4 `scripts/dev-check.ps1`

应自动检查：

- Docker 和 Compose 可用
- 所有容器处于 healthy
- PostgreSQL 可连接
- migration 版本正确
- 必需 bucket 存在
- NATS JetStream 可用
- `business-api /health/ready` 通过

---

## 9. 数据库迁移

### 9.1 迁移目录

```text
migrations/
├── 202607300001_create_tenants.sql
├── 202607300002_create_users.sql
├── 202607300003_create_documents.sql
├── 202607300004_create_jobs.sql
├── 202607300005_create_outbox.sql
└── ...
```

### 9.2 执行方式

建议提供独立 migration 应用：

```powershell
cargo run -p migration -- up
cargo run -p migration -- status
```

也可以使用：

```powershell
cargo sqlx migrate run
```

### 9.3 验收要求

必须验证：

```text
空数据库
→ 全部 migration
→ 成功

上一版本数据库
→ 增量 migration
→ 成功

重复执行
→ 不破坏数据

migration 失败
→ 应用发布停止

数据库版本不兼容
→ readiness 失败
```

禁止：

- 应用启动时临时创建业务表
- 手工执行未纳管 SQL
- 修改已发布 migration
- 依赖开发人员本地数据库已有状态

---

## 10. 基础设施抽象

### 10.1 Repository

领域层定义接口，基础设施层实现 PostgreSQL。

```rust
#[async_trait::async_trait]
pub trait ContractRepository: Send + Sync {
    async fn find_by_id(
        &self,
        tenant_id: TenantId,
        contract_id: ContractId,
    ) -> Result<Option<Contract>, RepositoryError>;

    async fn save(
        &self,
        contract: &Contract,
        expected_version: Version,
    ) -> Result<Version, RepositoryError>;
}
```

实现：

```text
PgContractRepository
FakeContractRepository
```

### 10.2 ObjectStorage

```rust
#[async_trait::async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn put(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        data: bytes::Bytes,
    ) -> Result<ObjectMetadata, StorageError>;

    async fn get(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<bytes::Bytes, StorageError>;

    async fn delete(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<(), StorageError>;

    async fn exists(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<bool, StorageError>;
}
```

实现：

```text
S3ObjectStorage
MemoryObjectStorage
```

### 10.3 MessageBus

```rust
#[async_trait::async_trait]
pub trait MessageBus: Send + Sync {
    async fn publish<T>(
        &self,
        subject: &str,
        message: &T,
    ) -> Result<(), MessagingError>
    where
        T: serde::Serialize + Send + Sync;
}
```

实现：

```text
NatsMessageBus
InMemoryMessageBus
```

### 10.4 AI Provider

```text
MockLlmProvider
HttpLlmProvider
MockOcrProvider
HttpOcrProvider
```

大多数自动化测试使用 Mock Provider，真实供应商兼容性测试单独执行。

---

## 11. 对象 Key 规范

推荐：

```text
tenant/{tenant_id}/documents/{document_id}/versions/{version}/source.pdf
tenant/{tenant_id}/documents/{document_id}/versions/{version}/ocr/result.json
tenant/{tenant_id}/documents/{document_id}/versions/{version}/parsed/document.json
tenant/{tenant_id}/exports/{export_id}/result.xlsx
```

禁止：

```text
documents/合同.pdf
uploads/file1.pdf
temp/result.json
```

要求：

- key 包含 tenant
- key 包含业务资源 ID
- key 包含版本
- 原始文件名只作为 PostgreSQL metadata
- 临时对象必须具备 TTL 和清理任务

---

## 12. 测试分层

### 12.1 单元测试

不启动 Docker 和网络依赖。

验证：

- 领域状态机
- 金额、日期和字段规则
- 权限决策
- ActionPlan
- 幂等键
- AI 结果校验
- 自动填充决策
- 任务状态转换
- 重试分类

使用：

```text
Fake Repository
MemoryObjectStorage
InMemoryMessageBus
Mock AI Provider
```

### 12.2 PostgreSQL 集成测试

必须使用真实 PostgreSQL，覆盖：

- migration
- CRUD
- 唯一约束
- 外键
- JSONB
- 事务提交和回滚
- 乐观锁
- 并发更新
- 行锁
- 时间和时区
- 多租户隔离
- Outbox 同事务写入
- 连接池上限
- 暂时不可用

### 12.3 对象存储契约测试

同一套测试分别运行于：

```text
MemoryObjectStorage
S3ObjectStorage + MinIO
```

覆盖：

- 上传、下载和删除
- 不存在对象
- Content-Type
- metadata
- checksum
- 中文文件名 metadata
- 特殊字符 key
- 大文件流式上传
- 分片上传
- 超时和取消
- 租户隔离
- 临时对象清理
- 预签名 URL，可选

### 12.4 消息系统集成测试

覆盖：

- Stream 和 Consumer 初始化
- 发布和消费
- Ack、Nak
- 至少一次交付
- 重复消息
- 延迟重试
- Consumer 重启
- Worker 崩溃
- 消息重放
- 幂等消费
- Dead-letter 策略

### 12.5 端到端测试

启动：

```text
business-api
business-worker
ai-worker
PostgreSQL
MinIO
NATS
Mock AI Server
```

验证：

```text
上传文档
→ MinIO 保存
→ PostgreSQL 创建 document
→ 创建 job
→ Outbox 发布任务
→ Worker 消费
→ Mock OCR
→ Mock LLM
→ Schema 校验
→ 保存 AI result
→ 生成填充建议
→ API 查询
→ 应用字段
→ 写入审计
```

异常场景必须包括：

- OCR 超时
- LLM 429
- LLM 非法 JSON
- Worker 中途崩溃
- 重复消息
- 重复提交
- 数据版本冲突
- 用户取消
- MinIO 暂时不可用
- PostgreSQL 暂时不可用

---

## 13. Testcontainers

Docker Compose 适合开发人员长期运行；Testcontainers 适合测试按需创建、自动销毁依赖。

建议开发依赖：

```toml
[dev-dependencies]
testcontainers = "0.27"
testcontainers-modules = { version = "0.15", features = ["postgres"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

进入项目时应统一锁定版本。

测试命令：

```powershell
cargo test --workspace --lib

cargo test --workspace --features integration-tests

cargo test --workspace --features e2e-tests
```

约定：

```text
默认 cargo test
→ 不启动 Docker

integration-tests
→ 启动 PostgreSQL、MinIO、NATS

e2e-tests
→ 启动完整系统
```

---

## 14. Mock AI Server

外部 LLM、OCR 和文档解析 API 不应成为大多数自动化测试的真实外部依赖。

Mock AI Server 应支持：

- OCR 成功
- OCR 超时
- OCR 错误
- LLM 合法结构化输出
- LLM 非法 JSON
- LLM 429
- LLM 500
- 慢响应
- 响应中断

可以通过测试 header 选择场景：

```http
X-Test-Scenario: llm-invalid-json
```

真实 AI API 测试作为单独的 `vendor-compatibility-tests`：

- 使用脱敏数据
- 设置费用上限
- 手动或定时运行
- 不阻塞普通提交
- 记录供应商协议变化

---

## 15. CI 流程

推荐：

```text
static
→ unit
→ integration
→ e2e
→ image-build
```

### 15.1 Static

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace --all-targets
```

### 15.2 Unit

```powershell
cargo test --workspace --lib
```

### 15.3 Integration

```powershell
cargo test --workspace --features integration-tests
```

### 15.4 E2E

```powershell
docker compose -f deploy/ci/compose.yml up -d
cargo run -p migration -- up
cargo test -p e2e-tests
docker compose -f deploy/ci/compose.yml down --volumes
```

测试失败后仍必须执行清理。

### 15.5 镜像验证

构建：

- business-api
- business-worker
- ai-worker
- agent-adapter
- migration

随后执行：

- 启动检查
- 健康检查
- 非 root 用户检查
- 漏洞扫描
- SBOM
- 许可证检查

---

## 16. 健康检查

### 16.1 Liveness

```text
GET /health/live
```

只检查进程和运行时是否存活。

```json
{
  "status": "alive"
}
```

### 16.2 Readiness

```text
GET /health/ready
```

检查：

- PostgreSQL 可连接
- migration 版本兼容
- 必需 bucket 存在
- NATS 可连接
- 必需配置完整
- 应用未进入 shutdown

```json
{
  "status": "ready",
  "dependencies": {
    "postgres": "ok",
    "object_storage": "ok",
    "messaging": "ok",
    "migrations": "ok"
  }
}
```

应用启动时可以检查 bucket，但不应在每个请求中创建 bucket。

---

## 17. 预生产环境

预生产环境必须：

- 与生产隔离
- 使用独立数据库
- 使用独立 bucket
- 使用独立消息系统
- 使用独立凭证
- 使用持久卷
- 具备监控
- 可执行备份和恢复
- 使用脱敏数据

最低拓扑：

```text
business-api × 1～2
business-worker × 1～2
ai-worker × 1～N
agent-adapter × 1，可选

PostgreSQL × 1
MinIO/S3 × 1
NATS JetStream × 1 或 3
OpenTelemetry Collector
Prometheus
Grafana
Loki
Tempo
```

禁止：

- 与生产共用数据库
- 与生产共用 bucket
- 使用生产密钥
- 使用未脱敏生产数据
- 省略恢复演练

---

## 18. PostgreSQL 预生产验证

功能：

- 从零 migration
- 增量 migration
- 约束、索引、JSONB
- 乐观锁和并发
- 事务和 Outbox
- 查询性能

故障：

- PostgreSQL 重启
- 短暂断网
- 连接池耗尽
- 长事务
- 死锁
- 慢查询
- migration 失败

运维：

- 定时备份
- 手工备份
- 恢复到新实例
- 数据完整性检查
- 凭证轮换
- 备份保留和加密

---

## 19. MinIO/S3 预生产验证

功能：

- bucket 和 policy
- 上传、下载、删除
- metadata
- 大文件和分片上传
- 预签名 URL
- 对象版本
- 生命周期

故障：

- 服务重启
- 网络中断
- 请求超时
- 认证失败
- 存储容量不足
- 上传中断
- bucket 不存在

运维：

- 凭证轮换
- bucket 备份
- 对象复制
- 恢复到新实例
- 对象数量检查
- checksum 抽检
- 历史版本恢复

---

## 20. 消息系统预生产验证

验证：

- 服务重启
- Consumer 重启
- Worker 崩溃
- 消息重复
- 消息积压
- 消息重放
- 延迟重试
- Dead-letter
- Ack 丢失
- 幂等消费
- Schema 版本兼容

---

## 21. 备份与恢复

### 21.1 目标

备份的验收标准不是“产生备份文件”，而是：

```text
在规定时间内
恢复到新环境
业务可以启动
数据一致性检查通过
```

### 21.2 PostgreSQL

预生产最低要求：

- 每日逻辑备份
- 保留 7～30 天
- 备份文件加密
- 备份结果校验
- 每月至少一次恢复演练

正式生产再根据恢复目标增加 WAL/PITR。

### 21.3 对象存储

可采用：

- MinIO replication
- `mc mirror`
- rclone
- 存储快照
- 异地 S3 bucket

最低要求：

- 对象文件备份
- bucket 配置备份
- 生命周期配置备份
- 对象数量检查
- checksum 抽样验证

### 21.4 PostgreSQL 与对象存储一致性

建议流程：

```text
上传临时对象
→ PostgreSQL 事务写入元数据
→ 事务成功后标记对象已提交
→ 失败时异步清理临时对象
```

对象状态：

```text
Temporary
Committed
PendingDelete
Deleted
Orphaned
```

定期扫描：

- 对象存在但数据库无记录
- 数据库记录存在但对象缺失
- 临时对象超过 TTL
- 删除任务长期失败

### 21.5 恢复顺序

```text
1. 恢复 PostgreSQL
2. 恢复对象存储
3. 恢复必要消息状态
4. 运行一致性扫描
5. 修复孤儿和缺失引用
6. 启动 worker
7. 启动 API
8. 执行验收测试
```

---

## 22. 故障注入

预生产必须主动执行：

- 停止 PostgreSQL
- 停止 MinIO
- 停止 NATS
- 杀死 business-worker
- 杀死 ai-worker
- 返回 AI 429
- 返回 AI 500
- AI 响应超时
- 制造重复消息
- 制造版本冲突
- 制造存储容量告警

验收：

- 不丢失权威业务状态
- 不重复执行正式写操作
- 临时错误可重试
- 永久错误进入 Failed
- Worker 恢复后可继续处理
- 用户可看到明确状态
- 监控产生告警
- 审计链条完整

---

## 23. 性能验证

PostgreSQL：

- P50/P95/P99
- 连接池
- 慢查询
- 索引命中
- 并发更新
- 热点行
- JSONB 查询
- 分页

对象存储：

- 小文件吞吐
- 大文件上传
- 并发上传和下载
- 流式传输内存
- 超时和重试

AI 流程：

- OCR 并发上限
- LLM 并发上限
- 外部限流
- Worker 扩容
- 任务积压
- 单文档耗时
- Token 和费用

---

## 24. 安全要求

本地开发：

- 仅绑定 `127.0.0.1`
- 使用开发专用密码
- `.env` 不提交 Git
- 不使用真实敏感数据
- 测试文件必须脱敏

预生产：

- 独立凭证
- TLS
- 最小权限
- Secret 管理
- bucket policy
- 数据库网络隔离
- 备份加密
- 禁止匿名对象访问

生产：

- 高可用
- 监控告警
- 凭证轮换
- 访问审计
- 定期漏洞扫描
- 备份恢复演练

---

## 25. 标准开发流程

```powershell
pwsh scripts/dev-up.ps1

cargo run -p migration -- up

cargo run -p business-api

cargo run -p business-worker

cargo test
```

集成测试：

```powershell
pwsh scripts/test-integration.ps1
```

完整 E2E：

```powershell
pwsh scripts/test-e2e.ps1
```

重建本地环境：

```powershell
pwsh scripts/dev-reset.ps1
```

---

## 26. 实施任务

### INFRA-01：本地 Compose

交付 PostgreSQL、MinIO、bucket 初始化、NATS、healthcheck 和 PowerShell 脚本。

### INFRA-02：统一配置

交付 development/test/staging 配置、环境变量覆盖、Secret 约束和配置校验。

### INFRA-03：SQLx Migration

交付 migration app、空库初始化、增量升级和版本检查。

### INFRA-04：ObjectStorage

交付 `ObjectStorage` trait、S3 实现、内存实现和 bucket 检查。

### INFRA-05：PostgreSQL 测试基座

交付 Testcontainers、独立测试数据库和自动 migration。

### INFRA-06：对象存储契约测试

交付 Memory 与 MinIO 共用测试集、大文件、metadata 和租户隔离测试。

### INFRA-07：消息测试基座

交付 Stream、Consumer、重复消息、重放和幂等消费测试。

### INFRA-08：Mock AI Server

交付 OCR 和 LLM 的成功、失败、超时、429 和非法响应场景。

### INFRA-09：完整 E2E

交付文档上传、OCR、LLM 抽取、结果保存、字段应用和审计流程。

### INFRA-10：CI

交付 static、unit、integration、e2e、镜像构建和清理。

### INFRA-11：预生产

交付持久化部署、Secret、监控、备份和恢复。

### INFRA-12：故障演练

交付数据库、对象存储、消息、Worker 和 AI API 故障验证报告。

---

## 27. 验收标准

本地环境：

- 一条命令启动
- healthcheck 全部通过
- 一条命令停止
- 一条命令重置
- Windows 环境可运行
- 不依赖共享服务

PostgreSQL：

- 空库 migration 通过
- 集成测试使用真实 PostgreSQL
- 乐观锁、事务和 Outbox 通过
- 中断恢复测试通过

MinIO/S3：

- 上传、下载和删除通过
- 大文件和流式上传通过
- 租户 key 隔离通过
- metadata 和 Content-Type 正确
- 中断后的任务状态正确

消息系统：

- 重复消息不产生重复业务结果
- Worker 崩溃后可恢复
- 消息积压可观测
- 重放和幂等通过

CI：

- 普通单元测试无需 Docker
- 集成测试自动创建容器
- E2E 自动启动完整环境
- 失败后自动清理
- 镜像版本固定

预生产：

- 完成备份
- 恢复到新实例
- 数据与对象一致性检查通过
- 故障注入通过
- 监控和告警可用
- 形成恢复演练记录

---

## 28. 架构决策摘要

1. 本地开发使用 Docker Compose。
2. 自动集成测试使用 Testcontainers。
3. 不使用 SQLite 替代 PostgreSQL 集成验证。
4. 不使用本地文件系统替代 S3 集成验证。
5. 生产对象存储保持可替换。
6. PostgreSQL 是权威业务状态。
7. 文档本体进入对象存储，数据库保存元数据和引用。
8. 数据库与对象存储通过状态机和一致性扫描协调。
9. 预生产必须执行备份恢复和故障演练。
10. 未验证恢复的备份不能视为有效备份。

---

## 29. 与总体架构的关系

本方案是《企业 AI 业务平台与智能助手总体架构方案_v2.md》的基础设施实施补充。

总体架构定义：

```text
PostgreSQL
MinIO/S3
NATS/Kafka
OpenTelemetry
```

本文定义：

```text
正式设施未部署时如何开发
如何自动验证真实依赖
如何建立 CI
如何建设预生产
如何备份和恢复
如何进行故障演练
```

后续应形成三层文档体系：

```text
总体架构方案
+
基础设施开发验证与预生产方案
+
正式生产部署、备份恢复和运维手册
```

---

## 30. 最终结论

正式 PostgreSQL 和 MinIO 尚未部署，不会阻塞 Rust 后端开发。

推荐路径：

```text
本地开发
→ Docker Compose

快速业务验证
→ Fake 和 Memory 实现

真实依赖验证
→ Testcontainers

完整流程验证
→ CI E2E

上线前
→ 独立持久化预生产

生产准入
→ 备份、恢复、故障和性能验收
```

最终原则：

> Fake 用于提高开发速度，真实基础设施用于证明正确性。

> 本地环境用于开发，Testcontainers 用于自动验证，预生产用于证明系统能够被部署、恢复和运维。

> 任何基础设施方案只有在备份、恢复和故障演练通过后，才能进入正式生产。
