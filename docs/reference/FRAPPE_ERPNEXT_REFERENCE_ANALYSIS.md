# Frappe Framework + ERPNext 源码级架构分析

> 文档类型：Reference Analysis  
> 状态：Current  
> 检查日期：2026-08-12  
> 研究对象：`frappe/frappe`、`frappe/erpnext` 官方 GitHub 源码  
> 固定提交：`frappe/frappe@21b840572497b02bc42e6bf842cd62e1abca4ddb`、`frappe/erpnext@ca1b03cd4647b1968f74256070c4d3453614d408`  
> 默认分支：两仓库均为 `develop`  
> 许可证边界：`frappe/frappe` = MIT，`frappe/erpnext` = GPL-3.0  
> 结论用途：作为 metadata-driven 平台设计的反例与参考，不作为本项目直接依赖决策

## 1. 结论摘要

Frappe Framework 是一个强 metadata-driven 的应用平台：大量系统行为不是写死在编译期，而是由 `hooks.py`、DocType JSON、Property Setter、Custom Field、Workspace/Report/Page/Web Form 元数据、以及运行时的 hook 解释器共同决定。ERPNext 则是建立在这个平台上的应用包，它通过同一套 hook 和文档元数据机制注入业务能力、桌面入口、打印/网页/报表/工作流/安装迁移行为。

这套机制的代价很明确：

- DDD 的边界会被“可动态拼装的元数据”侵蚀，业务规则更像平台配置而不是强类型领域代码；
- ownership 由“某个上下文拥有正式事实”退化为“某个 DocType/Workspace/Report 由谁写入和导出”；
- compile-time safety 很弱，很多约束只能在运行时、安装时、迁移时或请求时校验；
- isolation 依赖权限过滤、hook 筛选、模块屏蔽和运行时约束，而不是编译器级隔离。

## 2. 事实记录

### 2.1 仓库与许可证

**FACT**：`frappe/frappe` 的包元数据声明它是 `metadata driven, full-stack low code web framework`，`app_license` 和 `package.json` 许可证均为 `MIT`。  
证据：
- [`frappe/pyproject.toml`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/pyproject.toml)
- [`frappe/package.json`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/package.json)

**FACT**：`frappe/erpnext` 的包元数据声明 `GPL-3.0`，并在 `pyproject.toml` 中把 `frappe` 作为 bench 依赖，要求 `>=17.0.0-dev,<18.0.0`。  
证据：
- [`erpnext/pyproject.toml`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/pyproject.toml)
- [`erpnext/package.json`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/package.json)

**FACT**：Frappe 的 `hooks.py` 显式定义 `app_name = "frappe"`、`app_license = "MIT"`、`before_install`、`after_install`、`doctype_js`、`website_route_rules`、`permission_query_conditions`、`has_permission`、`doc_events`、`scheduler_events`、`before_migrate`、`after_migrate`、`override_whitelisted_methods` 等关键入口。  
证据：
- [`frappe/hooks.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/hooks.py)

**FACT**：ERPNext 的 `hooks.py` 定义 `add_to_apps_screen`、`doctype_js`、`doctype_list_js`、`page_js`、`extend_doctype_class`、`override_whitelisted_methods`、`after_install`、`after_app_install`、`after_app_uninstall`、`boot_session`、`filters_config`、`additional_print_settings`、`treeviews`、`website_generators`、`website_route_rules`、`standard_navbar_items`、`standard_portal_menu_items`、`webform_list_context`。  
证据：
- [`erpnext/hooks.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/hooks.py)

### 2.2 metadata 驱动的核心机制

**FACT**：Frappe 的 `get_hooks()` 会加载每个 app 的 `hooks.py`，并对 hook 值做聚合；`append_hook()` 会把 dict/list 结构归一化成可合并的运行时配置。  
证据：
- [`frappe/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/__init__.py)

**FACT**：`get_doc_hooks()` 会把 `doc_events` 合并并按 doctype 展开；`Document.hook()` 再在运行时把 controller 方法和 hook handlers 组合执行。  
证据：
- [`frappe/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/__init__.py)
- [`frappe/model/document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/document.py)

**FACT**：`Document.run_method()` 会调用 controller 方法、再调用 `run_webhooks()`、再调用 server script 事件。  
证据：
- [`frappe/model/document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/document.py)

**FACT**：`frappe.whitelist()` 把函数登记到全局白名单集合；`is_whitelisted()` 决定是否允许通过 HTTP/API 暴露。  
证据：
- [`frappe/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/__init__.py)

**FACT**：`frappe.api.__init__` 明确声明 API 版本化路由，支持 `/api/method/{methodname}`、`/api/resource/{doctype}`、`/api/resource/{doctype}/{name}`，以及 `v1` / `v2` 子路径。  
证据：
- [`frappe/api/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/__init__.py)
- [`frappe/api/v1.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/v1.py)
- [`frappe/api/v2.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/v2.py)

**FACT**：API discovery 会扫描 installed apps 下的 Python 文件、识别 `@frappe.whitelist`，并对 Doctype controller 的 whitelisted methods 生成 discovery 文档；`expose_discovery_source` hook 决定是否公开方法源码。  
证据：
- [`frappe/api/discovery.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/discovery.py)

**FACT**：`load_doctype_module()` 会按 app/module/doctypes 的 Python 包路径加载 controller；`override_doctype_class` 和 `extend_doctype_class` 允许替换/扩展 controller 类型。  
证据：
- [`frappe/modules/utils.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/modules/utils.py)
- [`frappe/model/base_document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/base_document.py)

### 2.3 DocType、Custom Field、Property Setter

**FACT**：Frappe 的对象模型以 DocType 元数据为核心，`frappe.get_meta()` 决定字段、表字段、权限、title field、workflow 等运行时行为。  
证据：
- [`frappe/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/__init__.py)
- [`frappe/model/document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/document.py)

**FACT**：`Custom Field` 是正式的 DocType 记录，不是临时 UI 状态。它的 `validate()`、`on_update()`、`on_trash()` 会校验字段冲突、触发 `updatedb`、清缓存，并且禁止给 core doctypes 加自定义字段。  
证据：
- [`frappe/custom/doctype/custom_field/custom_field.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/custom/doctype/custom_field/custom_field.py)

**FACT**：`Property Setter`/`Customize Form` 是对 DocType/DocField 的覆盖层，`Customize Form` 会创建和删除 `Custom Field`、`Property Setter`。  
证据：
- [`frappe/custom/doctype/customize_form/customize_form.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/custom/doctype/customize_form/customize_form.py)
- [`frappe/custom/doctype/property_setter/property_setter.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/custom/doctype/property_setter/property_setter.py)

**FACT**：`frappe.reload_doc()` / `frappe.reload_doctype()` 都只是把 JSON/文件系统中的模型重新同步到 DB。  
证据：
- [`frappe/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/__init__.py)
- [`frappe/modules/export_file.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/modules/export_file.py)

**INFERENCE**：这意味着“字段是否存在、是否可写、是否展示、是否可翻译、是否放进 list view”等核心 UI/模型事实，并不是编译器固化，而是可在站点运行时改写的元数据状态。

### 2.4 权限、事件、工作流

**FACT**：权限模型是运行时组合的：`has_permission()` 先看管理员/doctype 元数据/子表/角色权限，再看 `if_owner`、user permissions、share permissions 和 controller hook。  
证据：
- [`frappe/permissions.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/permissions.py)

**FACT**：`permission_query_conditions` 和 `has_permission` 都可通过 hook 覆盖某些 DocType 的访问逻辑。  
证据：
- [`frappe/hooks.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/hooks.py)

**FACT**：工作流不是纯状态机配置文件，而是 runtime model。`validate_workflow()` 会检查当前状态、下一状态、transition 合法性；`set_workflow_state_on_action()` 会根据 submit/cancel/update-after-submit 回写 workflow state。  
证据：
- [`frappe/model/workflow.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/workflow.py)
- [`frappe/model/document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/document.py)

**FACT**：`doc_events` 会把 `on_update`、`after_insert`、`on_cancel`、`on_trash`、`on_change` 等事件挂到具体 handler 上，包括 workflow action、assignment rule、webhook、search index、Google Calendar/Contacts 等副作用。  
证据：
- [`frappe/hooks.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/hooks.py)
- [`frappe/model/document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/document.py)

**INFERENCE**：Frappe 的“事件驱动”是元数据驱动的，不是编译期显式依赖图。多数副作用来自 hook 解释器、doc events、server script 和 runtime permission evaluation。

### 2.5 安装、卸载、迁移、fixtures

**FACT**：Frappe 的安装流程显式分为 `before_install`、`after_install`、`after_app_install`、`after_app_uninstall`；`before_install` 会先 reload 核心 DocType，`after_install` 会初始化标准文档、默认角色、语言、通知类型等。  
证据：
- [`frappe/utils/install.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/utils/install.py)
- [`frappe/hooks.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/hooks.py)

**FACT**：`before_migrate` / `after_migrate` 同样通过 hook 运行，且 `after_migrate` 会做主题、搜索索引、通知类型等重建。  
证据：
- [`frappe/hooks.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/hooks.py)
- [`frappe/core/doctype/patch_log/patch_log.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/core/doctype/patch_log/patch_log.py)

**FACT**：`frappe.utils.fixtures` 将 fixtures 导入/导出绑定为站点级迁移语义；`frappe.flags.in_fixtures` 会被运行时检查，避免在 fixtures 模式下触发不应发生的副作用。  
证据：
- [`frappe/utils/fixtures.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/utils/fixtures.py)
- [`frappe/desk/doctype/workspace/workspace.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/desk/doctype/workspace/workspace.py)

**FACT**：`export_to_files()` 会把文档导出成模块目录下的 JSON/代码文件；`Workspace.on_update()` 在开发模式下会导出 workspace 到模块文件。  
证据：
- [`frappe/modules/export_file.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/modules/export_file.py)
- [`frappe/desk/doctype/workspace/workspace.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/desk/doctype/workspace/workspace.py)

**INFERENCE**：安装、卸载、迁移、fixtures 并不是“数据库迁移工具”的附属功能，而是这个平台的正式状态变更机制。

### 2.6 Workspace、Page、View、Desk 贡献

**FACT**：Workspace 是一个正式 DocType，且其 controller 直接控制内容、角色、app mount、导出和删除逻辑。  
证据：
- [`frappe/desk/doctype/workspace/workspace.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/desk/doctype/workspace/workspace.py)

**FACT**：Desk 的运行时会把 Workspace、Page、Report、Module Def、App hook、权限和模块屏蔽合并成 `bootinfo`。  
证据：
- [`frappe/boot.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/boot.py)
- [`frappe/desk/desktop.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/desk/desktop.py)

**FACT**：`get_workspaces()` 和 `get_sidebar_items()` 会把 Workspace/Sidebar Item/Module/App/Permission 组合成最终 desk payload；`default_workspace_map` 决定同一个实体路由到哪个 workspace sidebar。  
证据：
- [`frappe/boot.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/boot.py)
- [`frappe/public/js/frappe/ui/sidebar/sidebar.js`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/public/js/frappe/ui/sidebar/sidebar.js)

**FACT**：`get_mountable_apps()`、`add_to_apps_screen`、`add_to_workspace_dock` 把“哪个 app 拥有哪个 workspace / 该 workspace 进入哪个 dock rail”变成运行时可配置关系。  
证据：
- [`frappe/desk/doctype/workspace/workspace.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/desk/doctype/workspace/workspace.py)
- [`frappe/boot.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/boot.py)

**FACT**：ERPNext 在 `hooks.py` 里通过 `add_to_apps_screen` 把自己挂到 apps screen；通过 `website_route_rules`、`webform_list_context`、`website_generators`、`standard_navbar_items`、`standard_portal_menu_items` 给桌面与网站层注入内容。  
证据：
- [`erpnext/hooks.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/hooks.py)

**INFERENCE**：Page/View/Workspace/Report 不是静态前端页面，而是平台内的“可挂载视图对象”，其归属、可见性和默认导航都由 metadata + boot payload 决定。

### 2.7 REST/API 生成与公开契约

**FACT**：Frappe API 的实现分三层：`/api/method` RPC、`/api/resource` REST、`/api/v2/document` 以及 discovery 文档。  
证据：
- [`frappe/api/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/__init__.py)
- [`frappe/api/v1.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/v1.py)
- [`frappe/api/v2.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/v2.py)

**FACT**：`v2` 的 Doctype method route 会把 `GET` 映射成 `read`，`POST` 映射成 `write`，并且只公开 whitelisted 且符合 Doctype 约束的方法。  
证据：
- [`frappe/api/v2.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/v2.py)

**FACT**：API discovery 会基于源码和 controller 反射生成 method index，并可在 app 显式 opt-in 时暴露函数源码。  
证据：
- [`frappe/api/discovery.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/discovery.py)

**INFERENCE**：Frappe 的“API 生成”不是按接口定义编译出固定 SDK，而是对运行时可见的 whitelisted 方法、controller 方法和 server scripts 做反射式公开。

### 2.8 ERPNext 的 app packaging 与跨 app 依赖

**FACT**：ERPNext 的 packaging 入口在 `erpnext/hooks.py`、`pyproject.toml` 和 `package.json`，其中 `hooks.py` 直接声明桌面、网页、安装、卸载、扩展 controller、webform context、nav items、treeviews、routes。  
证据：
- [`erpnext/hooks.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/hooks.py)
- [`erpnext/pyproject.toml`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/pyproject.toml)
- [`erpnext/package.json`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/package.json)

**FACT**：ERPNext 在 Python 侧通过 `allow_regional()`、`check_app_permission()`、`normalize_ctx_input()` 等 helper 进一步把“地区覆盖、应用权限、输入规范化”做成可插拔层。  
证据：
- [`erpnext/__init__.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/__init__.py)

**FACT**：ERPNext 的 install/uninstall 逻辑会 seed 标准 navbar、安装通知类型、创建/删除 desktop icons，并通过 app title 计算卸载时需要清理的桌面图标。  
证据：
- [`erpnext/setup/install.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/setup/install.py)

**FACT**：ERPNext 的 `boot_session()` 将业务级上下文注入 boot payload。  
证据：
- [`erpnext/startup/boot.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/startup/boot.py)

**INFERENCE**：ERPNext 不是“独立于 Frappe 的业务系统”，而是一个深度依赖 Frappe runtime、hook registry、metadata store 和 desk boot 流的 app pack。

## 3. 机制分析

### 3.1 metadata-driven 的具体机制

Frappe/ERPNext 的 metadata-driven 机制可以概括为：

1. app 级 hook 文件声明可插拔入口；
2. DocType JSON / Custom Field / Property Setter 决定模型形状；
3. controller / doc_events / server script 在 runtime 绑定行为；
4. boot payload 将权限过滤后的 workspace/page/report/module/app 结构一次性送到前端；
5. API discovery 再把可见方法反射成机器可读目录。

这个结构不是“配置替代代码”这么简单，而是“代码、数据、行为、导航、API 共同共享同一套元数据源”。

### 3.2 对 DDD 的牺牲

**INFERENCE**：DDD 里的“一个 Bounded Context 拥有自己的模型、聚合和语言”在这里会被 metadata 层稀释，因为很多事实不是在领域类里静态声明，而是通过 hooks、Custom Field、Property Setter、workspace JSON、workflows、reports、web forms 运行时组装。

**INFERENCE**：上下文边界不是强编译边界，而是：

- app/module/package 边界；
- hook 约定边界；
- 运行时权限过滤边界；
- 站点级安装/迁移边界。

因此它更像“可组合平台”而不是“强隔离领域模型”。

### 3.3 对 ownership 的牺牲

**INFERENCE**：正式 ownership 不是仅靠代码目录决定，而是由：

- `DocType.module`
- `Workspace.module` / `Workspace.app`
- `Module Def.app_name`
- `custom` / `standard` 标记
- `app_title` / `add_to_apps_screen`
- `after_install` / `after_app_uninstall`

共同决定。

这使得“谁拥有事实”变成运行时规则，而不是编译器保证的单一所有者关系。

### 3.4 对 compile-time safety 的牺牲

**INFERENCE**：Frappe/ERPNext 的不少约束只能在运行时发现：

- 字段是否存在；
- custom field 是否与 core doctype 冲突；
- workflow transition 是否允许；
- 权限 query condition 是否正确；
- controller method 是否 whitelisted；
- hook 是否写错路径；
- workspace/app mount 是否存在于已安装 app；
- export/import path 是否可写。

这意味着大部分错误不会被静态类型系统或编译器拦住，而是在请求/安装/迁移/保存时暴露。

### 3.5 对 isolation 的牺牲

**INFERENCE**：系统隔离依赖：

- 权限检查；
- `blocked_modules` / `disabled_modules`；
- app hook opt-in；
- read-only / install / migrate / fixtures flags；
- cache/boot payload 过滤；
- server script 读取权限。

它不是“模块不可见 = 编译不可引用”的隔离。只要有运行时反射和动态路径，隔离就更多依赖约定和防护，而不是语言级封闭。

## 4. 具体源码路径清单

### Frappe Framework

- [`frappe/hooks.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/hooks.py)
- [`frappe/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/__init__.py)
- [`frappe/model/document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/document.py)
- [`frappe/model/base_document.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/base_document.py)
- [`frappe/model/workflow.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/model/workflow.py)
- [`frappe/permissions.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/permissions.py)
- [`frappe/modules/utils.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/modules/utils.py)
- [`frappe/api/__init__.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/__init__.py)
- [`frappe/api/v1.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/v1.py)
- [`frappe/api/v2.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/v2.py)
- [`frappe/api/discovery.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/api/discovery.py)
- [`frappe/custom/doctype/custom_field/custom_field.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/custom/doctype/custom_field/custom_field.py)
- [`frappe/custom/doctype/customize_form/customize_form.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/custom/doctype/customize_form/customize_form.py)
- [`frappe/custom/doctype/property_setter/property_setter.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/custom/doctype/property_setter/property_setter.py)
- [`frappe/utils/install.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/utils/install.py)
- [`frappe/utils/fixtures.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/utils/fixtures.py)
- [`frappe/modules/export_file.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/modules/export_file.py)
- [`frappe/desk/doctype/workspace/workspace.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/desk/doctype/workspace/workspace.py)
- [`frappe/desk/desktop.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/desk/desktop.py)
- [`frappe/boot.py`](https://github.com/frappe/frappe/blob/21b840572497b02bc42e6bf842cd62e1abca4ddb/frappe/boot.py)

### ERPNext

- [`erpnext/hooks.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/hooks.py)
- [`erpnext/__init__.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/__init__.py)
- [`erpnext/setup/install.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/setup/install.py)
- [`erpnext/startup/boot.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/startup/boot.py)
- [`erpnext/controllers/website_list_for_contact.py`](https://github.com/frappe/erpnext/blob/ca1b03cd4647b1968f74256070c4d3453614d408/erpnext/controllers/website_list_for_contact.py)

## 5. Project Decision

**PROJECT DECISION**：如果本项目要吸收 Frappe/ERPNext 的优点，应该只吸收“metadata 可编译、可重建、可挂载、可审计”的平台组织方式，不吸收它的动态模型失真。具体来说：

- 业务事实必须有明确 Bounded Context 和单一权威所有者；
- 运行时自定义只允许作为受控 overlay，而不是正式事实本体；
- hook、workflow、permission、workspace、API discovery 必须保留，但要被严格收敛到显式 contract；
- 不把 runtime 组装当成领域模型本身；
- 不把站点级 metadata 改写当成编译期安全的替代品。

这样才能保留平台灵活性，同时避免把“可配置”误当成“可验证”。

## 6. 备注

- 本次研究未修改任何代码。
- 仅新增本文件：`docs/reference/FRAPPE_ERPNEXT_REFERENCE_ANALYSIS.md`。
- 若后续需要，我可以继续把这份分析压缩成一页版，或者拆成“Frappe Framework 机制”和“ERPNext app packaging”两个独立参考文件。
