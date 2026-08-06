# 项目执行计划

本目录保存可执行、可验收、有结束条件的实施计划。

## 目录

```text
plans/
├── README.md
├── current/    正在执行或下一步已批准/已提出的计划
└── archive/    已完成、取消或被替代的计划
```

## 规则

1. `current` 只保留仍然有效的计划；
2. 一个计划必须有目标、非目标、步骤、风险、测试和完成定义；
3. 计划不能修改长期架构基线；需要改变基线时先创建 ADR；
4. 完成、取消或被替代后必须归档；
5. 归档时记录最终提交、验收和未完成项；
6. 不把长期参考资料或架构正文放入计划目录；
7. `Proposed` 计划只表示下一步候选，不代表已经授权进入实现。

## 当前计划

- [`current/PLAN-0006-enterprise-ai-workspace-foundation.md`](current/PLAN-0006-enterprise-ai-workspace-foundation.md)：`Proposed`，建立 Workspace、Skill/Context/Tool Registry、任务级 Capability、Observation 和只读业务助手垂直切片；尚未激活实现。

## 已归档计划

- [`archive/2026/PLAN-0001-foundation-hardening.md`](archive/2026/PLAN-0001-foundation-hardening.md)：`Integrated`，服务基座与首个垂直切片加固。
- [`archive/2026/PLAN-0002-foundation-integrity-and-closeout.md`](archive/2026/PLAN-0002-foundation-integrity-and-closeout.md)：`Integrated`，基础设施完整性和收口。
- [`archive/2026/PLAN-0003-persistence-query-architecture.md`](archive/2026/PLAN-0003-persistence-query-architecture.md)：`Integrated`，持久化、查询与多数据库架构。
- [`archive/2026/PLAN-0004-durable-document-processing-mvp.md`](archive/2026/PLAN-0004-durable-document-processing-mvp.md)：`Integrated`，持久化文档处理、Lease/Fence 和恢复。
- [`archive/2026/PLAN-0005-runtime-audit-integrity-repair.md`](archive/2026/PLAN-0005-runtime-audit-integrity-repair.md)：`Integrated`，Runtime Audit、Integrity 和 Controlled Repair。

## 归档

归档路径按年份组织：

```text
archive/2026/PLAN-XXXX-*.md
```

文档生命周期遵循 [`../governance/DOCUMENT_MANAGEMENT.md`](../governance/DOCUMENT_MANAGEMENT.md)。
