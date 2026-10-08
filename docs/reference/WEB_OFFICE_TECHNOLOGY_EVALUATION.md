# Web Office 技术选型初步调研

> 文档类型：Reference（非架构决策）
> 状态：Draft / 待 PoC 验证
> 调研日期：2026-10-08
> 适用场景：Business Platform 合同及企业文档管理、Office 在线预览/编辑、Agent 辅助文档操作
> 责任范围：Document Management / Document Intelligence / Enterprise AI Workspace 的候选外部能力；不改变现有数据所有权

## 1. 结论摘要（非 Accepted 决策）

- **短期首选验证 ONLYOFFICE Docs**：现成 Web Office，优先测试 DOCX/XLSX/PPTX 的预览、编辑、保存、协作与复杂合同排版。其社区版采用 AGPL-3.0 及附加条款，商业 SaaS 的实际嵌入与分发模式须先做法律审查；此处不是生产准入或采购决策。
- **长期优先调研 Univer**：浏览器原生 SDK 与命令/Facade API 更适合自定义 Agent 驱动的 Office 界面；但开源核心与 Univer Pro 有明确边界，Office 格式导入/导出、协同等关键功能不能默认为免费。
- **GenOffice 作为源码和文件级编辑引擎参考**：Electron/React 桌面应用，具备 TypeScript 的 DOCX/PPTX 引擎、局部 OOXML 修改思路和 Rust XLSX sidecar。未验证整套应用可直接部署为 Web；Web 迁移需单独评估。
- **Collabora Online/CODE 作为排版保真对照**：可用于复杂合同、跨格式兼容性和高保真显示对比；CODE 官方定位为开发/测试版本，不建议用于生产。
- 上述仅为**候选优先级**。未进行真实合同样本 PoC、部署压测、Agent 安全评估或法律结论；**不新增 ADR、Active Plan 或已批准的部署单元**。

## 2. 当前项目约束

1. Document Management 拥有文档身份、生命周期、内容修订和存储引用；Document Intelligence 拥有处理作业、抽取候选、证据与人工复核；对象存储只拥有文件字节。参见 [Durable Document Processing Architecture](../architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md)。
2. 当前固定文档处理 MVP 接受纯文本、Markdown、JSON；PDF、图片、Office 和压缩包当前返回 `unsupported_content_type`。在线 Office 选型**不等于**处理流水线已支持 Office 解析。
3. 企业 AI Workspace 的 Artifact/Preview、Capability 与 Agent 写操作必须遵循现有权限、租户、版本、审计、`Prepare → Preview → Confirm → Execute` 约束；Office 引擎和 Agent 不能成为业务内容版本的权威来源。
4. 实施顺序应服从 [当前计划](../plans/README.md) 的 PLAN-0014 与 Contract Business Vertical Slice，PoC 不自行激活新服务或扩大通用平台范围。

## 3. 选项矩阵（截至 2026-10-08 的源码/文档初查）

| 选项 | 交付形态 / 对 Office 的处理 | Agent 集成点 | 许可与商业边界 | 初步定位 |
|---|---|---|---|---|
| [ONLYOFFICE Docs](https://github.com/ONLYOFFICE/DocumentServer) | 独立 Web Document Server；Office 文档预览、编辑、多人协同；可嵌入应用 | 编辑器 API、插件及外围受控文档命令；外部直接控制编辑器的高级 API 须另查版本授权 | Community 为 AGPL-3.0，且须审核仓库 LICENSE 的附加条款；官网 README 表述为「up to 20 recommended」，**不是可据此推导的固定法定用户上限** | **P0：短期 PoC 第一候选** |
| [Univer](https://github.com/dream-num/univer) | Web SDK：Sheets、Docs 等开放核心，组件/命令式嵌入 | Facade API、插件、命令/模型层；适合精细化 Agent 操作 | 开源核心 Apache-2.0；协同、Office 导入/导出、部分 Slides/高级功能属于 Pro/商用范围 | **P0：原生 Web/Agent 路线第二候选** |
| [GenOffice](https://github.com/genspark-ai/genoffice) | Electron + React 桌面应用；TS 文档引擎和 Rust XLSX sidecar，尚非现成 Web 服务 | Agent Core、CLI/MCP、编辑差异/局部修改技术参考 | 主体 Apache-2.0；`ee/` 单独企业许可；迁移时须核对所有分包和品牌授权 | **P1：源码复用及 Web 化可行性研究** |
| [Collabora Online](https://github.com/CollaboraOnline/online) | LibreOffice 技术路线的在线 Office；集成时评估 WOPI/部署模式 | 适配层或外围文档命令；不是预设的 Agent 原生操作引擎 | CODE 开发版无生产 SLA，官方不建议生产；商业支持版另行评估 | **P1：保真度对照组** |

**证据与注意事项：**

- ONLYOFFICE 的官方仓库目前将 Community、Enterprise、Developer 分为不同版本，Community README 写的是「up to 20 recommended」，不能把它直接写成硬性 20 并发许可限制。AGPL 使用和 SaaS/闭源应用的集成义务依赖具体修改、部署、组合和分发方式，需法务逐案判断，不作“整个 SaaS 必须开源”或“必然无需开源”的一刀切断言。
- Univer 官方明确区分 OSS 与 Pro：其基础编辑、Facade 和插件系统属开源范围，但许多导入导出与协作功能在 Pro。评估必须按**拟采用的具体包与版本**确认，而非按产品概述推断。
- GenOffice 官方说明 `packages/*` 为与 Electron 分离的 TypeScript 引擎；Sheets 的 Rust sidecar、Electron IPC、文件系统和 UI 桥接意味着 **React 可复用 ≠ 完整 Web Office 可直接部署**。局部 OOXML patch 尽量保留未触及内容，不代表复杂排版已经被证明 100% 无损。
- Collabora 官方明确 CODE 不面向有稳定支持要求的生产环境，生产方案应另核商业支持版本。

## 4. 与 Business Platform 的候选集成边界

以下是**待 ADR/PoC 确认的设计方向**，不修改现有 Baseline：

- Browser：React/Next.js 合同详情中嵌入候选 Office 编辑器；不让浏览器持有 S3 密钥、Agent 服务凭证或长效下载地址。
- Document Management：始终负责原件、版本、内容引用、访问授权、保存/修订生效与审计；文档服务只提供编辑会话或处理能力。
- 外部 Office 引擎：通过可替换的适配边界获得短时受控读取和保存回调。保存完成并不直接代表业务数据已正式提交，仍须经过拥有者应用用例、乐观锁、幂等与审计。
- Agent：优先采用受控、高层语义文档命令（读取选中范围、建议替换、填写字段、产生候选版本与预览）；不暴露任意 Shell、文件系统、SQL、URL 或未经授权的编辑器自动化能力；敏感写操作沿用现有 Prepare/Preview/Confirm/Execute。
- 安全：租户与资源授权先于会话创建；临时访问地址、防 SSRF/内网回环、回调签名与重放保护、恶意 OOXML/宏/外链、审计与数据驻留均需验证。
- 不因为某引擎使用 Node/Electron/Rust 就预先创建新的微服务；先通过外部受控服务/适配器 PoC，是否正式独立部署后续评审。

## 5. 最小 PoC 与客观验收建议（尚未激活）

1. **样本集**：使用脱敏合同 DOCX（分页、表格、页眉/页脚、目录、批注、修订记录、字体和嵌入对象）、财务 XLSX（公式、格式、多 Sheet）及少量 PPTX；原始文件不可被覆盖。
2. **同一测试矩阵**：对 ONLYOFFICE、Univer（明确 OSS / Pro 包界限）、Collabora 分别验证打开、只读预览、局部编辑、保存、重新打开、与源文件的布局/数据一致性；GenOffice 先验证 DOCX round-trip 与浏览器迁移依赖，不与现成 Web 服务直接混为一类。
3. **保真与数据正确性**：页级截图人工/自动对照；抽查复杂字体、分页、域、批注、修订记录、内容控件；XLSX 公式与数值一致；重要结构不得丢失。保存失败、断连和并发冲突必须有可恢复、可观察的结果。
4. **Agent 安全与业务控制**：无权限租户访问失败；Agent 不能直接提交正式文件覆盖旧版本；要求授权的写操作能预览、确认、审计并定位到具体文档修订；重复保存回调保持幂等。
5. **成本/运维/许可**：对比服务内存、CPU、启动依赖、并发、无 Docker 本地环境的替代测试部署路径、升级成本和 SaaS 授权。商业许可审核结论未明确前不得标记“可用于闭源生产 SaaS”。
6. **决策产物**：PoC 产出真实样本证据、适配差距、许可意见与推荐；必要时创建 Proposed ADR、独立实施计划，再考虑进入架构 Baseline。

## 6. 当前状态与明确非目标

- **仅调研**：未选择生产供应商；未签署或采购许可证；未验证文件保真；未部署任何候选 Office 服务。
- **不改变**：当前 Document Management / Document Intelligence 所有权、固定 MVP 输入类型、公开 API/Event、数据库、业务计划优先级。
- **不承诺**：全面取代 Microsoft Office/ONLYOFFICE、自研完整排版引擎、直接将 GenOffice 迁入浏览器或将全部 OSS SDK 视为免费 Office 文件引擎。

## 7. 官方来源与检查基线

以下修订为 2026-10-08 获取的公开仓库分支 HEAD，仅用于本次事实核对；实际 PoC 应固定测试版本与 digest：

| 项目 | 检查分支 / Revision | 源码与明确依据 |
|---|---|---|
| ONLYOFFICE DocumentServer | `master` @ `f580eb58439432310943ece02c9730c6a21365e7` | [仓库与版本矩阵](https://github.com/ONLYOFFICE/DocumentServer)、[LICENSE 附加条款](https://github.com/ONLYOFFICE/DocumentServer/blob/master/LICENSE)、[集成示例](https://github.com/ONLYOFFICE/document-server-integration) |
| Univer | `dev` @ `6615a129d9446cabfc34f85557351a74f309176b` | [仓库 OSS vs Pro 功能矩阵](https://github.com/dream-num/univer) |
| GenOffice | `main` @ `b08e2ebf7204e28c94eb3fdcaf285891bab002a2` | [仓库 README 与 License 说明](https://github.com/genspark-ai/genoffice)、[模块架构](https://github.com/genspark-ai/genoffice/blob/main/CONTRIBUTING.md)、[ee 许可](https://github.com/genspark-ai/genoffice/blob/main/ee/LICENSE) |
| Collabora Online | `main` @ `5bce075cee48496de3e817ea73d76fd2a37d289e` | [源码](https://github.com/CollaboraOnline/online)、[官方 CODE/生产说明](https://www.collaboraonline.com/faqs/) |

关联本项目：[Document Management](../architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md)、[Enterprise AI Workspace](../architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md)、[SaaS Platform](../architecture/SAAS_PLATFORM_ARCHITECTURE.md)、[ADR-0018](../adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md)、[ADR-0025](../adr/ADR-0025-enterprise-ai-saas-platform-and-independent-modules.md)。

> 本文是后续选型/PoC 的起点；任何“采用/采购/架构定案”必须基于新增验证证据和对应治理流程。
