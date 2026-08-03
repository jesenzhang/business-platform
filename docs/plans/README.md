# 项目执行计划

本目录保存可执行、可验收、有结束条件的实施计划。

## 目录

```text
plans/
├── README.md
├── current/    正在执行或下一步已批准的计划
└── archive/    已完成、取消或被替代的计划
```

## 规则

1. `current` 只保留仍然有效的计划；
2. 一个计划必须有目标、非目标、步骤、风险、测试和完成定义；
3. 计划不能修改长期架构基线；需要改变基线时先创建 ADR；
4. 完成、取消或被替代后必须归档；
5. 归档时记录最终提交、验收和未完成项；
6. 不把长期参考资料或架构正文放入计划目录。

## 当前计划

- [`archive/2026/PLAN-0001-foundation-hardening.md`](archive/2026/PLAN-0001-foundation-hardening.md)：`Integrated`，修正初始骨架的安全和架构阻断项，建立可验证的服务基座。

## 归档

归档路径按年份组织：

```text
archive/2026/PLAN-XXXX-*.md
```

文档生命周期遵循 [`../governance/DOCUMENT_MANAGEMENT.md`](../governance/DOCUMENT_MANAGEMENT.md)。
