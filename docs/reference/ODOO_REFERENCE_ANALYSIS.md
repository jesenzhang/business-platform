# Odoo 源码级架构研究

> 检查日期：2026-08-12
> 仓库：`odoo/odoo`
> 默认分支：`19.0`
> 固定 exact commit：[`2f0f8e5e00685129b5bbe954117bc9f80a568e88`](https://github.com/odoo/odoo/tree/2f0f8e5e00685129b5bbe954117bc9f80a568e88)
> 研究边界：只基于官方 GitHub 源码与仓库元数据，不复制外部代码
> 许可证边界：仓库根目录 [`LICENSE`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/LICENSE) 明示 LGPLv3；[`COPYRIGHT`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/COPYRIGHT) 说明多数文件归 Odoo S.A.，部分第三方内容保留各自许可证；GitHub API 元数据对该仓库返回 `Other/NOASSERTION`，因此本研究以源码树内许可证文本为准，不能仅靠仓库级元数据决定复用范围

## 证据索引

- [`README.md`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/README.md) - 仓库定位、产品边界、官方入口
- [`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py) - manifest 解析、发现、默认值、外部依赖、post_load
- [`odoo/modules/module_graph.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module_graph.py) - 依赖图、加载顺序、phase/depth/order
- [`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py) - install/upgrade/uninstall 主流程
- [`odoo/modules/migration.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/migration.py) - pre/post/end migration 约定
- [`odoo/addons/base/__manifest__.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/__manifest__.py) - `base` 特例 manifest
- [`odoo/addons/base/models/ir_model.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_model.py) - xmlid、数据所有权、卸载清理
- [`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py) - 视图继承、arch_db/arch_fs、校验与组合
- [`odoo/addons/base/models/ir_rule.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_rule.py) - 记录规则、group/global 语义
- [`odoo/addons/base/models/ir_module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_module.py) - 模块状态机与管理 UI
- [`addons/account/__manifest__.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/addons/account/__manifest__.py) - 代表性业务模块 manifest
- [`addons/mail/__manifest__.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/addons/mail/__manifest__.py) - 代表性业务模块 manifest

## Facts

- **FACT**：Odoo 在仓库自述中定义为“web based open source business apps”套件，既支持单模块独立使用，也支持多模块组合成 ERP。来源：[`README.md`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/README.md)
- **FACT**：`base` 是特殊模块。`odoo/addons/base/__manifest__.py` 显式声明 `depends: []`、`auto_install: True`、`post_init_hook`，并包含 `security/*`、`views/*`、`wizard/*`、`data/*` 等核心资源。来源：[`odoo/addons/base/__manifest__.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/__manifest__.py)
- **FACT**：`odoo/modules/module.py` 只认 `__manifest__.py`，会为 manifest 生成默认字段；当模块不是 `base` 且未声明 depends 时，会默认补成 `['base']`；`author`、`license`、`version` 也会被校验或补默认值。来源：[`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py)
- **FACT**：manifest 支持 `data`、`demo`、`assets`、`external_dependencies`、`pre_init_hook`、`post_init_hook`、`post_load`、`uninstall_hook` 等生命周期字段。来源：[`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py)
- **FACT**：`addons/account/__manifest__.py` 与 `addons/mail/__manifest__.py` 展示了典型业务模块模式：声明 `depends`，再按 `security`、`data`、`views`、`wizard`、`demo`、`assets` 分桶组织资源，并通过 `application`、`post_init_hook`、`installable`、`license` 描述模块属性。来源：[`addons/account/__manifest__.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/addons/account/__manifest__.py), [`addons/mail/__manifest__.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/addons/mail/__manifest__.py)
- **FACT**：`odoo/modules/module_graph.py` 通过依赖图计算模块加载顺序，固定 `base` 在 phase 0，再按 `phase / depth / order_name` 排序；`to install`、`to upgrade`、`to remove` 的状态会影响 phase 计算。来源：[`odoo/modules/module_graph.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module_graph.py)
- **FACT**：`odoo/modules/loading.py` 将模块生命周期拆成明确步骤：先加载 `base`，再更新模块列表，随后执行 install/upgrade/reinit，加载数据与 demo，执行迁移脚本，验证视图与约束，最后处理卸载与 registry 重建。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)
- **FACT**：`odoo/modules/migration.py` 规定升级脚本目录结构必须是 `migrations/<version>/pre-*.py`、`post-*.py`、`end-*.py`；`0.0.0` 表示跨版本脚本，且 `migrate(cr, installed_version)` 是强约束签名。来源：[`odoo/modules/migration.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/migration.py)
- **FACT**：`ir.ui.view` 以 `inherit_id`、`mode`、`arch_db`、`arch_fs`、`arch_prev`、`arch_updated`、`group_ids` 表达视图继承与来源；其计算逻辑会在文件模式与数据库模式之间切换，并对 inheritance specs 做校验。来源：[`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py)
- **FACT**：`ir.rule` 把记录规则分成 `global` 与 group 规则；无 group 时为 global，group 规则按当前用户 group 过滤并与 global 组合；`perm_read/perm_write/perm_create/perm_unlink` 决定在哪些操作上生效。来源：[`odoo/addons/base/models/ir_rule.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_rule.py)
- **FACT**：`ir.model` 负责 xmlid、模型元数据和卸载清理；`_process_end` 会清理模块更新后遗留的 xmlid/记录，`_module_data_uninstall` 会按模块移除外部标识、模型、字段、约束与依赖视图。来源：[`odoo/addons/base/models/ir_model.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_model.py)
- **FACT**：`ir.module.module` 是模块状态机的正式载体，状态包括 `uninstalled`、`installed`、`to install`、`to upgrade`、`to remove`、`uninstallable`。来源：[`odoo/addons/base/models/ir_module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_module.py)

## Architecture

- **INFERENCE**：Odoo 的架构是“模块化单体 + 运行时注册表 + 声明式资源加载”，不是把模块当作完全独立的插件服务。`manifest` 负责声明，`module_graph` 负责顺序，`registry` 负责运行时组装，`ir.model.data` 负责公开标识与后续清理。来源：[`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py), [`odoo/modules/module_graph.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module_graph.py), [`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py), [`odoo/addons/base/models/ir_model.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_model.py)
- **INFERENCE**：Odoo 把“模块依赖”与“数据加载顺序”绑定在同一套图上，因此安装、升级、重初始化都不是单纯的文件复制，而是依赖图重算、模型初始化、数据导入、迁移脚本、校验和清理的一整套事务性过程。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py), [`odoo/modules/module_graph.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module_graph.py)
- **INFERENCE**：`ir.ui.view` 不是普通 CRUD 记录，而是一个“组合层”。数据库中的 `arch_db`、文件中的 `arch_fs`、继承链、可见 group、翻译、调试与恢复都被放进同一模型语义里。来源：[`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py)
- **INFERENCE**：Odoo 的安全边界分成两层：第一层是模型 ACL / group / record rule 的静态约束，第二层是具体业务对象上的运行时访问检查与异常消息。`ir.rule` 负责筛选数据范围，`ir.model` / `ir.module.module` / `ir.ui.view` 负责在安装和更新时修正、清理、校验。来源：[`odoo/addons/base/models/ir_rule.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_rule.py), [`odoo/addons/base/models/ir_module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_module.py)

## Mechanisms

### manifest 与依赖

- **FACT**：`Manifest._from_path` 只读取 `__manifest__.py`，并用 `ast.literal_eval` 解析；`load_openerp_module` 先 import 模块，再执行 `post_load` 钩子。来源：[`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py)
- **FACT**：`depends` 的默认值对 `base` 特殊处理，非 `base` 且未声明依赖时会被补成 `['base']`；`auto_install` 可以是布尔值，也可以是依赖集合。来源：[`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py)
- **FACT**：`external_dependencies` 会检查 Python 包和 PATH 可执行文件，缺失时抛出显式错误。来源：[`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py)

### registry 与模型装配

- **FACT**：`load_modules` 先把 `base` 放进依赖图，再调用 `load_module_graph`；之后才处理额外的 install/upgrade/reinit 请求。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)
- **FACT**：`load_module_graph` 会在 install/upgrade 时执行 `pre_init_hook`、模块导入、`registry.load(package)`、`registry.init_models(...)`，然后导入数据并执行 `post_init_hook` 或升级后的视图校验。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)
- **FACT**：模块安装/升级后，Odoo 会检查新模型是否缺少 access rules，并给出生成 `ir.model.access.csv` 的提示。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)

### 视图继承

- **FACT**：视图定义支持从文件恢复 `arch_fs`，也支持从数据库 `arch_db` 读写；当视图来自 XML 文件时，Odoo 会尝试保存相对路径以便后续 hard reset。来源：[`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py)
- **FACT**：视图继承通过 inheritance specs 和 XPath locator 验证实现；无效 locator 会被显式标记出来，而不是默默吞掉。来源：[`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py)

### 安全

- **FACT**：`ir.rule` 的 global/group 语义会影响最终 domain；record rule 的 domain 用安全求值后再用 `Domain.validate` 校验。来源：[`odoo/addons/base/models/ir_rule.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_rule.py)
- **FACT**：`ir.module.module` 使用管理员权限装配模块 UI 与状态变更，模块管理不是普通业务 CRUD。来源：[`odoo/addons/base/models/ir_module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_module.py)

### 迁移

- **FACT**：迁移脚本必须命名为 `pre-*`、`post-*`、`end-*`，并放在版本目录里；`0.0.0` 脚本用于跨版本场景。来源：[`odoo/modules/migration.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/migration.py)
- **FACT**：`MigrationManager` 只对需要升级的模块收集脚本，按 installed_version 与 current_version 比较后再执行。来源：[`odoo/modules/migration.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/migration.py)

### 安装 / 升级 / 卸载

- **FACT**：安装时会按图加载依赖，执行 pre-init、数据导入、demo 导入、post-init，最后把模块状态写回 `installed`。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)
- **FACT**：升级时会先跑 pre-migration，再导入更新数据，再跑 post-migration，最后执行 end-migration、视图校验、约束修复和 orphan 清理。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py), [`odoo/modules/migration.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/migration.py)
- **FACT**：卸载时会先执行 `uninstall_hook`，再调用 `module_uninstall()`，最后重建 registry 并清理 `ir.model.data`、模型、字段、约束和复制视图。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py), [`odoo/addons/base/models/ir_model.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_model.py)

## Strengths

- **INFERENCE**：Odoo 的优势是“声明式、可排序、可验证、可回滚”的模块生命周期，特别适合大量业务应用共享同一平台内核的场景。来源：[`odoo/modules/module_graph.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module_graph.py), [`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)
- **INFERENCE**：视图 inheritance + xmlid + validation 使 UI 变更具备较强的可组合性与可追踪性，出错时能定位到具体 locator。来源：[`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py)
- **INFERENCE**：记录规则与 group 语义分离，使“谁能看见什么”可以在模型层统一表达，而不是散落在控制器和视图里。来源：[`odoo/addons/base/models/ir_rule.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_rule.py)
- **INFERENCE**：migration 约定把升级脚本变成显式、版本化、可审计的代码，而不是把升级逻辑藏在启动魔法里。来源：[`odoo/modules/migration.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/migration.py)

## Failure modes

- **INFERENCE**：模块之间通过 `_inherit`、xmlid、视图继承和共享基础模型形成深耦合，单个模块的升级可能在别处引入不可见副作用。来源：[`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py), [`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)
- **INFERENCE**：如果视图 locator、record rule domain 或 manifest 版本不正确，失败通常是在加载期才暴露，问题定位成本高。来源：[`odoo/addons/base/models/ir_ui_view.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_ui_view.py), [`odoo/addons/base/models/ir_rule.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_rule.py), [`odoo/modules/module.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/module.py)
- **INFERENCE**：卸载并不只是删记录，而是要删模型、字段、约束、外部标识、复制视图并可能重启 registry；这意味着卸载路径复杂，且最容易暴露历史包袱。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py), [`odoo/addons/base/models/ir_model.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/addons/base/models/ir_model.py)
- **INFERENCE**：`init_xml` 已被标记为 deprecated，说明 manifest 演化存在历史兼容债务；新系统若照搬会把旧负担一起带进去。来源：[`odoo/modules/loading.py`](https://github.com/odoo/odoo/blob/2f0f8e5e00685129b5bbe954117bc9f80a568e88/odoo/modules/loading.py)

## Adopt / Adapt / Reject / Defer

| 设计模式 | 决策 | 理由 | 对 business-platform 的含义 |
|---|---|---|---|
| 声明式 `__manifest__.py` | Adopt | 以文件描述模块元数据、依赖、数据桶和生命周期钩子，易于静态校验与排序 | 作为 `BusinessModuleManifest` / 能力清单参考 |
| `depends` + 依赖图排序 | Adopt | 把模块装配变成可重算图问题，而非手写顺序 | 作为模块依赖解析与装配顺序基础 |
| `base` 特例 | Adapt | Odoo 把 `base` 当作引导内核；本项目不一定需要完全等价特例，但需要有“平台内核模块”概念 | 为平台内核模块保留单独规则，但不要让特例蔓延 |
| `data` / `demo` / `security` / `views` 分桶 | Adopt | 资源分层清晰，利于安装、测试和审计 | 作为模块包目录与加载阶段参考 |
| `post_load` / `pre_init_hook` / `post_init_hook` / `uninstall_hook` | Adapt | 这是有用的生命周期钩子，但应受边界约束，避免成为任意代码入口 | 只在明确的扩展点启用，禁止隐式热修补 |
| `MigrationManager` 的 `pre/post/end` 版本脚本 | Adopt | 升级逻辑需要显式、版本化、可审计 | 作为升级/回滚脚本规范 |
| `ir.model.data` 的 xmlid  ownership | Adapt | 其“发布标识 + 清理”思想可借鉴，但不能直接变成我们的业务权威数据存储 | 作为发布资源与外部引用层参考 |
| `ir.ui.view` 的 XML 继承模型 | Adapt / Defer | 如果业务平台继续使用 XML UI，可借鉴；若 UI 体系不同，则不要强行移植 | 只把“组合/覆盖/验证”原则迁移，不照搬实现 |
| `ir.rule` 的 group/global 规则 | Adapt | 规则模型可借鉴，但必须与我们的租户、权限、数据分类体系一致 | 作为策略引擎设计输入 |
| `init_xml` / `update_xml` 历史机制 | Reject | 19.0 已对 `init_xml` 给出弃用信号，说明这是历史兼容包袱 | 不纳入新平台设计 |
| 直接把 Odoo ORM / registry 作为平台核心实现 | Reject | 这会把 Odoo 的耦合、数据模型和升级语义一并引入 | 仅做参考，不作为实现基座 |

## Mapping to business-platform

| Odoo 机制 | business-platform 映射 | 说明 |
|---|---|---|
| `__manifest__.py` | `BusinessModuleManifest` | 描述模块能力、依赖、数据包、生命周期钩子 |
| `depends` / `module_graph` | 模块依赖解析器 | 负责装配顺序与闭包校验 |
| `registry` | 运行时注册表 / 组件装配结果 | 承载已解析模块与模型集合 |
| `ir.model.data` / xmlid | 公开资源标识层 | 允许稳定引用已发布对象，但不暴露内部存储细节 |
| `ir.ui.view` | 视图组合引擎 | 若采用 XML UI，则继承/组合/校验都应显式化 |
| `ir.rule` | 策略引擎 | 表达对象级访问控制与数据范围 |
| `MigrationManager` | 版本化迁移执行器 | 负责 pre/post/end 升级脚本与兼容性演进 |
| `post_init_hook` / `uninstall_hook` | 生命周期钩子 | 仅在明确边界内允许 |
| `base` 模块 | 平台核心模块 | 应有独立引导路径与最小稳定内核 |

## 结论

- **PROJECT DECISION**：在 business-platform 中，只吸收 Odoo 的“模块声明 + 依赖图 + 版本化迁移 + 显式生命周期 + 组合式 UI + 记录规则”这些结构性思想，不吸收其 ORM、视图语法、历史兼容包袱或运行时热装配方式。
- **PROJECT DECISION**：把 `manifest` 视为编译输入，把 `registry` 视为装配结果，把 `ir.model.data` 视为发布标识层，而不是把它们当作业务事实的权威存储。
- **PROJECT DECISION**：如果未来在 business-platform 中引入类似 Odoo 的模块体系，必须保留可验证的依赖排序、可审计迁移脚本、严格的数据所有权和独立的安全边界。
