# 项目执行计划

本目录保存可执行、可验收、有结束条件的实施计划。

## 目录

~~~text
plans/
├── README.md
├── current/    正在执行或仍然有效的下一步计划
└── archive/    已完成、取消或被替代的计划
~~~

## 规则

1. current 只保留仍然有效的计划；
2. 一个计划必须有目标、非目标、边界、步骤、风险、测试和完成定义；
3. 计划不能静默修改长期架构基线；需要改变基线时先通过 ADR；
4. 完成、取消或被替代后必须归档；
5. 归档时记录最终提交、验收和未完成项；
6. 不把长期参考资料或架构正文放入计划目录；
7. Proposed 只表示候选，不代表已经授权编码；
8. 优先用真实业务垂直切片验证平台抽象，禁止在没有第二/第三业务需求证据时继续扩大通用 Runtime。

## 当前计划与顺序

1. [PLAN-0013 Identity and Authorization Foundation](current/PLAN-0013-identity-and-authorization-foundation.md)：Proposed / NEXT CANDIDATE。实现外部 OIDC 之后的平台内用户映射、Tenant Membership、最小组织结构、Role/Permission/RoleBinding/Resource Scope 与统一 Policy Decision，为 Contract 和 Agent 建立真实授权基础。
2. **Contract Business Vertical Slice**：下一计划，在 PLAN-0013 集成后创建并激活。目标是 Contract List/Detail → Document Revision → AI Extraction/Evidence → Review → Apply Candidate → Contract Version Update，验证现有 Document/Module/Policy/UI 基础。
3. [PLAN-0006 Enterprise AI Workspace Foundation](current/PLAN-0006-enterprise-ai-workspace-foundation.md)：Proposed / Revision 1 / BLOCKED。在 Identity/Authorization 与首个 Contract 真实切片完成后，交付 Workspace/Turn/AgentRun、Compiled Tool Catalog、Capability、Observation 和只读业务助手。
4. 后续依次考虑 Knowledge/Evidence Projection、Analytics/Semantic Runtime、Approval/Finance/Legal 跨上下文切片；Platform Module Runtime、Marketplace、动态插件、Generated App Sandbox 不作为近期优先项。

## 已归档计划

- [PLAN-0001](archive/2026/PLAN-0001-foundation-hardening.md)：Integrated。
- [PLAN-0002](archive/2026/PLAN-0002-foundation-integrity-and-closeout.md)：Integrated。
- [PLAN-0003](archive/2026/PLAN-0003-persistence-query-architecture.md)：Integrated。
- [PLAN-0004](archive/2026/PLAN-0004-durable-document-processing-mvp.md)：Integrated。
- [PLAN-0005](archive/2026/PLAN-0005-runtime-audit-integrity-repair.md)：Integrated。
- [PLAN-0007](archive/2026/PLAN-0007-business-console-and-external-access-demo.md)：Integrated / Archived。
- [PLAN-0008](archive/2026/PLAN-0008-document-lifecycle-revision-and-evidence-foundation.md)：Integrated。
- [PLAN-0009](archive/2026/PLAN-0009-c-legacy-contract-and-document-migration-rehearsal.md)：Completed / Rehearsal Closed / Archived；production migration NOT GRANTED。
- [PLAN-0010](archive/2026/PLAN-0010-business-module-isolation-and-semantic-contract-foundation.md)：Integrated。
- [PLAN-0011](archive/2026/PLAN-0011-business-application-packaging-and-contribution-foundation.md)：Integrated / Archived。
- [PLAN-0012](archive/2026/PLAN-0012-runnable-v1-auth-ai-provider-observability.md)：Integrated / Archived；v0.1 预生产发布，merge 2383651，Main CI 33705531597，v0.1 tag → 2383651。

归档路径按年份组织，文档生命周期遵循 [DOCUMENT_MANAGEMENT.md](../governance/DOCUMENT_MANAGEMENT.md)。
