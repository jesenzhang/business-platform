# 服务端安全架构

> 文档 ID：ARCH-SEC-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：身份、租户、授权、数据保护、外部输入、Agent 与运维安全

## 1. 安全原则

1. 默认拒绝，明确授权；
2. 身份、租户和权限在进入业务用例前建立；
3. 业务上下文拥有业务授权语义；
4. 所有外部输入均不可信；
5. Secret、敏感数据和高风险操作最小暴露；
6. 安全决策可审计、可测试、可撤销；
7. Agent 和 AI 不获得高于原用户的权限；
8. 基础设施故障不能降级为绕过安全。

## 2. 信任边界

```text
Internet / Client
    ↓
Reverse Proxy / Gateway
    ↓
Delivery Layer
    ↓
Application Security Context
    ↓
Domain / Ports
    ↓
Infrastructure Adapters
    ↓
Database / Storage / Broker / Providers
```

外部系统、Agent Runtime、AI/OCR Provider、文件和消息内容均位于不可信边界之外。

## 3. 身份模型

内部统一使用 `Principal`：

- User Principal；
- Service Principal；
- Agent Delegated Principal；
- Support/Operations Principal。

调用上下文至少包含：

- principal_id；
- tenant_id；
- authentication_method；
- roles/attributes 或授权引用；
- delegation chain；
- request_id/trace_id；
- issued_at/expiry。

业务层不得解析 Token 或依赖外部身份供应商 DTO。

## 4. 认证

正式环境优先采用 OIDC/OAuth2 或公司统一身份平台。

要求：

- 验证签名、issuer、audience、expiry 和 nonce/状态；
- Key rotation；
- 生产禁止无签名 Token 和开发万能用户；
- 服务到服务使用独立身份，不复用人员凭证；
- 认证失败不泄漏内部原因；
- session/token 撤销策略明确。

## 5. 租户隔离

租户是强安全边界。

必须在以下层级验证：

- Delivery：提取并验证 tenant；
- Application：调用上下文与目标资源一致；
- Repository：查询和写入默认携带 tenant 条件；
- 数据库：唯一约束和索引包含 tenant，必要时使用 RLS 作为纵深防御；
- Object Key：包含受控 tenant segment；
- Cache：key 包含 tenant；
- Message/Event：包含 tenant context；
- Observability：避免跨租户敏感内容聚合泄漏。

不得仅依赖前端传递的 tenant_id。

## 6. 授权

采用 RBAC + ABAC + 业务状态规则组合：

```text
身份和角色
+ 资源属性
+ 租户和组织关系
+ 业务对象当前状态
+ 操作风险等级
= 最终授权决定
```

Policy Context 提供通用授权机制；具体业务上下文负责业务状态和不变量。

禁止：

- Handler 中散落权限判断；
- 仅用前端按钮控制权限；
- 使用数据库查询失败代替授权；
- Agent 自行推断权限。

## 7. 高风险操作

高风险写操作采用：

```text
Prepare → Preview → Confirm → Execute
```

ActionPlan/Confirmation 必须绑定：

- 用户和租户；
- 目标资源及版本；
- 操作摘要；
- 风险等级；
- 预计影响；
- 过期时间；
- 一次性 nonce；
- 原始请求或 trace。

目标版本变化、确认过期或主体变化后必须重新准备。

## 8. 数据分类

至少分为：

- Public；
- Internal；
- Confidential；
- Restricted。

合同、财务、身份凭证、个人信息和生产 Secret 通常属于 Confidential 或 Restricted。

每类数据定义：

- 可访问角色；
- 日志和 trace 策略；
- 加密要求；
- 导出和分享限制；
- 保留和删除；
- 测试环境使用限制。

## 9. 加密

### 传输

- 正式环境所有外部和跨主机通信使用 TLS；
- 校验证书和主机名；
- 不允许生产跳过证书验证。

### 静态

- 数据库磁盘、备份和对象存储启用受控加密；
- 应用级字段加密用于特别敏感数据；
- 密钥与数据分离管理；
- 支持轮换和版本。

## 10. Secret 管理

Secret 不进入：

- Git；
- 普通配置文件；
- Debug/Display；
- 日志和 trace；
- 错误响应；
- CI Artifact；
- 测试快照。

生产通过环境注入、Secret Manager 或平台 Secret 能力提供。

Secret 类型必须脱敏，并建立自动化泄漏测试。

## 11. 文件与对象安全

- 文件类型通过内容检测和允许列表验证，不只信任扩展名；
- 限制大小、页数、压缩比和解压深度；
- 防止 Zip Bomb、路径穿越和恶意嵌套文件；
- ObjectKey 使用受控值对象；
- 上传区和正式区分离；
- 必要时进行病毒/恶意内容扫描；
- 下载使用短期授权或受控流式代理；
- 不提供匿名公共 Bucket；
- 文件访问必须关联租户和业务资源授权。

## 12. AI、OCR 与 Prompt Injection

所有模型和文档输出视为不可信候选数据。

要求：

- 外部内容不能改变系统指令和权限；
- 模型不能直接获得数据库、Shell、文件系统或任意 HTTP；
- Tool 调用使用白名单、结构化 Schema 和服务端校验；
- AI 结果执行类型、范围、证据和业务规则验证；
- Prompt 和响应按数据分类脱敏；
- 记录模型、Prompt 版本和调用 trace，但不无条件记录完整敏感内容；
- 外部 Provider 数据处理范围和保留策略必须明确。

## 13. Agent 安全

Agent 是委托入口，不是新身份边界。

- 传播原用户身份和租户；
- 权限不超过原用户；
- 不向 Agent 暴露通用 SQL、Shell、任意文件系统或任意 HTTP；
- 只暴露业务级 Skill；
- 高风险写入需要预览和确认；
- Agent Runtime 不持有业务数据库凭证；
- Agent 的计划、调用和结果必须审计。

## 14. API 安全

- Body Limit；
- Timeout；
- Rate Limit 和配额；
- CORS 白名单；
- 安全 Header；
- 稳定错误码，不泄漏底层详情；
- Idempotency-Key；
- 乐观锁；
- 防止 Mass Assignment；
- 输入 Schema 和语义校验；
- 防止 ID 枚举泄漏；
- 导出接口额外权限和审计。

## 15. 消息安全

- Broker 使用认证和 TLS；
- subject/topic 权限最小化；
- 消息包含最少必要数据；
- 敏感大内容使用 ArtifactRef；
- 消费者验证 Schema、租户、来源和事件版本；
- 重复和伪造消息不能绕过业务用例；
- Dead Letter 内容受访问控制和保留策略保护。

## 16. 数据库安全

- 应用使用最小权限账户；
- Migration 使用独立高权限账户；
- 禁止运行时应用拥有随意 DDL 权限；
- SQL 参数化；
- 管理访问受网络和身份限制；
- 审计高风险管理操作；
- 备份加密并限制访问；
- 生产数据库不直接暴露公网。

## 17. 运维与支持访问

支持和运维访问必须：

- 使用个人身份；
- 最小权限和时限；
- 必要时双人审批；
- 全量审计；
- 禁止共享账号；
- 禁止通过直接改表完成业务处理；
- 提供受控管理用例和修复工具。

## 18. 安全事件响应

需要定义：

- 事件分级；
- 告警渠道；
- 凭证撤销；
- 账户冻结；
- 数据影响范围查询；
- 日志保全；
- 修复、通知和复盘；
- 安全事件后的密钥轮换。

## 19. 安全测试

至少包括：

- 未认证和越权；
- 跨租户；
- IDOR；
- 路径穿越；
- Secret 泄漏；
- Mass Assignment；
- 重复确认和过期确认；
- 事件伪造和重复；
- 恶意文件；
- Prompt Injection；
- 生产配置 fail-closed；
- 依赖漏洞和许可证扫描。

## 20. 威胁建模

新增外部入口、高风险业务、文件处理、Agent Tool、第三方集成或新数据类别时，必须进行轻量威胁建模：

```text
资产
→ 信任边界
→ 威胁
→ 控制
→ 剩余风险
→ 验证
```

重大安全边界变化通过 ADR。

## 21. 验收清单

- [ ] 认证、租户和授权上下文完整；
- [ ] 业务权限不散落在 Handler；
- [ ] Secret 不可通过日志和 Debug 泄漏；
- [ ] 文件、AI 和外部消息视为不可信；
- [ ] 高风险写入使用绑定版本的确认；
- [ ] Agent 权限不超过用户；
- [ ] 数据库、Broker 和对象存储最小权限；
- [ ] 安全测试和威胁模型随功能更新；
- [ ] 生产配置默认拒绝不安全降级。
