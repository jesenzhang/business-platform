# 外部参考项目

本目录保存对外部项目、产品和技术方案的事实性研究。参考材料用于形成架构输入，不能直接覆盖本项目的 Baseline、ADR、数据所有权或安全边界。

## 使用规则

1. 记录检查日期、来源、许可证和版本/提交；
2. 区分外部项目事实、项目适配分析和本项目正式决策；
3. 明确哪些能力采用、改造采用、延后或拒绝；
4. 长期架构变化必须进入 `docs/architecture/` 并通过 ADR；
5. 实现顺序和验收标准必须进入 `docs/plans/current/`。

## 已登记项目

| 项目 | 分类 | 主要参考价值 | 分析文档 |
|---|---|---|---|
| Cloudflare OS | 企业 AI Workspace / Agent 应用平台 | Workspace、Gatekeeper、Gadget、Blueprint、Capability-based security、Observation/Observer | [`CLOUDFLARE_OS_REFERENCE_ANALYSIS.md`](CLOUDFLARE_OS_REFERENCE_ANALYSIS.md) |
