# Clash Verge Rev 命令行完整控制 GUI 功能技术方案

## 1. 背景与目标

参考 `/mnt/c/Users/zxy10/Documents/Codex/2026-06-15/clash-verge/outputs/clash-verge-cli-control-summary.md` 的结论，普通命令行目前只能覆盖 Mihomo Core API、代理端口和少量配置文件修改，不能完整复刻 Clash Verge Rev GUI 的应用层行为。GUI 功能大量通过前端 `invoke(...)` 调用 Rust Tauri commands，再由后端执行配置写入、核心重启、系统代理、托盘刷新、服务安装、备份同步等副作用。

本方案目标是新增一套命令行控制能力，让用户可以用 CLI 完整控制 Clash Verge Rev GUI 中可操作的功能，并保证 CLI 操作与 GUI 操作共享同一套业务逻辑、状态和副作用。

目标能力：

- 控制 Mihomo 核心：启动、停止、重启、模式切换、运行时配置、日志、连接、代理组和节点。
- 控制 Verge 应用设置：系统代理、TUN、端口、语言、主题、热键、托盘、轻量模式、日志、自动启动、外部控制器等。
- 控制 Profile：列表、创建、导入、切换、更新、删除、排序、查看和保存配置文件。
- 控制系统能力：服务安装/卸载/修复、UWP 工具、打开目录、退出/重启应用。
- 控制备份：本地备份、WebDAV 配置、上传、列表、恢复、删除、导入导出。
- 提供脚本友好的 JSON 输出、稳定退出码、幂等操作和 dry-run。

非目标：

- 不用 CLI 重新实现一套 GUI 业务逻辑。
- 不把控制接口默认暴露到局域网或公网。
- 不把直接修改 YAML 文件作为完整控制方案的主路径。

## 2. 现状分析

当前项目的关键事实：

- `src-tauri/src/lib.rs` 通过 `tauri::generate_handler!` 注册大量 Tauri commands。
- `src/services/cmds.ts` 基本是前端对 Tauri command 的薄包装。
- `src-tauri/src/cmd/*` 是 GUI invoke 的入口层，例如 `patch_verge_config`、`patch_clash_config`、`start_core`、`restart_core`、`install_service`、`create_local_backup`。
- `src-tauri/src/feat/*` 已经沉淀了一部分可复用业务逻辑，例如 `feat::patch_verge`、`feat::patch_clash`、`feat::update_profile`、`feat::switch_proxy_node`、`feat::create_local_backup`。
- `src-tauri/src/utils/server.rs` 已有只监听 `127.0.0.1` 的 embedded server，但当前主要服务于单例唤醒、scheme 处理和 PAC，不适合作为未认证的完整控制 API 直接扩展。
- `src-tauri/src/core/handle.rs` 依赖全局 `APP_HANDLE` 发送前端事件并获取 `tauri-plugin-mihomo` 客户端，说明部分能力必须在已运行 GUI 进程内执行，不能由第二个进程无脑并发写配置。

因此完整方案必须解决两个问题：

1. CLI 如何调用正在运行的 GUI 后端。
2. GUI 未运行时，CLI 能执行哪些安全的 headless 操作，哪些必须提示启动 GUI 或失败。

## 3. 总体架构

推荐采用“内置 CLI + 本地 IPC Bridge + Headless fallback”的组合架构。

```text
clash-verge / clash-verge-cli
  |
  |-- GUI 已运行：通过本地 IPC 请求 GUI 进程执行
  |       |
  |       `-- GUI 后端复用 service/feat/core/config 逻辑
  |
  `-- GUI 未运行：进入 headless runtime
          |
          `-- 只执行无需 AppHandle、托盘、窗口、前端事件的离线操作

GUI 前端
  |
  `-- invoke("...") -> cmd::* -> service/feat/core/config

Mihomo Core
  |
  |-- tauri-plugin-mihomo LocalSocket
  `-- external-controller HTTP API，作为核心能力的补充路径
```

核心原则：

- CLI 和 GUI 不各自实现业务逻辑，而是共同调用同一套 Rust service。
- GUI 已运行时，CLI 优先把请求交给 GUI 进程执行，避免两个进程同时操作配置、托盘、系统代理和核心进程。
- Headless 模式只作为补充，主要支持读取状态、修改离线配置、导入/导出备份等不依赖 GUI 运行态的功能。

## 4. 模块拆分

新增或调整以下 Rust 模块：

```text
src-tauri/src/
  cli/
    mod.rs              # CLI 入口、参数解析、输出和退出码
    commands.rs         # CLI command 到业务 command 的映射
    output.rs           # human/json 输出格式
    confirm.rs          # 危险操作确认、--yes、--dry-run
    client.rs           # IPC client，连接 GUI bridge
    headless.rs         # GUI 未运行时的初始化和受限执行

  ipc/
    mod.rs              # 本地控制协议
    server.rs           # GUI 进程内 IPC server
    auth.rs             # token 生成、读取、校验
    schema.rs           # request/response/event schema

  service/
    mod.rs              # 从 cmd/feat 中抽出的复用服务
    app.rs
    clash.rs
    verge.rs
    profile.rs
    proxy.rs
    backup.rs
    webdav.rs
    service.rs
    runtime.rs
```

调整原则：

- `cmd::*` 只保留 Tauri 参数适配和错误转换。
- `cli::*` 只保留命令行参数适配、输出和退出码。
- `ipc::*` 只负责协议、认证和转发。
- `service::*` 承载 GUI 和 CLI 共享的真实业务逻辑。
- 现有 `feat::*` 可以逐步迁移到 `service::*`，第一阶段也可以由 service 包装现有 feat 函数，降低改动风险。

## 5. 入口设计

### 5.1 内置子命令

在 `src-tauri/src/main.rs` 中，在创建 Tauri GUI 前解析参数：

```text
clash-verge cli status
clash-verge cli core restart
clash-verge cli setting set enable_system_proxy true
```

如果第一个参数是 `cli`，进入 CLI runtime，不启动 WebView。

优点：

- 不需要额外安装二进制。
- 和现有打包流程耦合最少。

### 5.2 独立二进制

在 `src-tauri/Cargo.toml` 增加一个 bin：

```text
src-tauri/src/bin/clash-verge-cli.rs
```

命令形式：

```text
clash-verge-cli status
clash-verge-cli profile switch <id-or-name>
```

优点：

- 脚本体验更自然。
- 避免 `clash-verge cli ...` 和 GUI 启动参数混杂。

推荐落地顺序：

1. 第一阶段实现内置 `clash-verge cli ...`，便于共用打包产物。
2. 第二阶段增加 `clash-verge-cli` 独立二进制或安装别名。

## 6. IPC Bridge 设计

### 6.1 传输层选择

推荐优先级：

1. Windows：Named Pipe。
2. macOS/Linux：Unix Domain Socket。
3. fallback：仅监听 `127.0.0.1` 的 HTTP server。

原因：

- Named Pipe/Unix Socket 更适合本机私有控制接口。
- 可以通过文件权限或 pipe ACL 限制当前用户访问。
- 避免和 Mihomo external-controller 混淆。

现有 `utils::server::embed_server` 可以保留单例/PAC职责。完整 CLI Bridge 建议放到新的 `ipc::server`，不要和现有 `/commands/visible`、`/commands/pac` 混在一起，避免认证模型和生命周期互相污染。

### 6.2 协议

请求：

```json
{
  "id": "01J...",
  "method": "verge.patch",
  "params": {
    "enable_system_proxy": true
  },
  "dry_run": false
}
```

响应：

```json
{
  "id": "01J...",
  "ok": true,
  "data": {
    "changed": true
  },
  "warnings": []
}
```

错误：

```json
{
  "id": "01J...",
  "ok": false,
  "error": {
    "code": "permission_denied",
    "message": "administrator permission is required",
    "details": {}
  }
}
```

### 6.3 认证

首次启用 CLI Bridge 时生成随机 token：

```text
<app-home>/cli-token
```

要求：

- token 至少 256 bit。
- 文件权限限制为当前用户可读写。
- CLI client 读取 token 后放入 IPC 请求 header 或 request envelope。
- 支持 `clash-verge cli token rotate` 轮换 token。
- 默认不允许远程访问，不支持监听 `0.0.0.0`。

### 6.4 生命周期

GUI 启动时：

1. 初始化配置、日志、core manager。
2. 启动 IPC server。
3. 写入 server endpoint 文件，例如 `<app-home>/cli-endpoint.json`。
4. GUI 退出时删除 endpoint 或关闭 socket。

CLI 执行时：

1. 读取 endpoint。
2. 尝试连接 GUI IPC。
3. 连接成功则发送请求，由 GUI 进程执行。
4. 连接失败则检测 GUI 是否未运行。
5. 对允许 headless 的命令进入 headless fallback。
6. 对必须运行态的命令返回明确错误，并提示可先启动 GUI。

## 7. Command 覆盖范围

### 7.1 App

```text
clash-verge cli app status [--json]
clash-verge cli app dir
clash-verge cli app logs-dir
clash-verge cli app core-dir
clash-verge cli app restart
clash-verge cli app quit
clash-verge cli app lightweight on|off|status
```

映射现有能力：

- `get_app_dir`
- `open_app_dir`
- `open_logs_dir`
- `open_core_dir`
- `restart_app`
- `exit_app`
- `entry_lightweight_mode`
- `exit_lightweight_mode`
- `get_running_mode`

### 7.2 Core / Clash

```text
clash-verge cli core info
clash-verge cli core start|stop|restart
clash-verge cli core mode rule|global|direct
clash-verge cli core config get [--json|--yaml]
clash-verge cli core config patch --file patch.yaml
clash-verge cli core logs [--follow]
clash-verge cli core change <mihomo|mihomo-alpha|custom>
```

映射现有能力：

- `get_clash_info`
- `start_core`
- `stop_core`
- `restart_core`
- `patch_clash_mode`
- `patch_clash_config`
- `get_runtime_config`
- `get_runtime_yaml`
- `get_clash_logs`
- `change_clash_core`

Mihomo external-controller 可作为补充实现：

- 代理组列表。
- 节点选择。
- 连接列表和关闭连接。
- Provider 更新。

但 CLI 的主路径仍应优先通过 GUI IPC 使用 `tauri-plugin-mihomo` 和现有后端状态。

### 7.3 Proxy

```text
clash-verge cli proxy groups [--json]
clash-verge cli proxy select <group> <node>
clash-verge cli proxy delay <node> [--url URL] [--timeout MS]
clash-verge cli proxy tray-sync
clash-verge cli connection list
clash-verge cli connection close [--all|--id ID]
```

需要补齐 service：

- 复用 `feat::switch_proxy_node`。
- 从 `tauri-plugin-mihomo` 包装 proxies、providers、connections API。
- CLI 操作节点后触发前端事件和托盘刷新，与 GUI 一致。

### 7.4 Profile

```text
clash-verge cli profile list [--json]
clash-verge cli profile current
clash-verge cli profile switch <id-or-name>
clash-verge cli profile import <url-or-file> [--name NAME]
clash-verge cli profile create --file FILE --name NAME
clash-verge cli profile update <id-or-name>
clash-verge cli profile delete <id-or-name> --yes
clash-verge cli profile edit <id-or-name> --set key=value
clash-verge cli profile read <id-or-name>
clash-verge cli profile save <id-or-name> --file FILE
clash-verge cli profile reorder <active-id> <over-id>
```

映射现有能力：

- `get_profiles`
- `create_profile`
- `import_profile`
- `update_profile`
- `delete_profile`
- `patch_profile`
- `patch_profiles_config`
- `patch_profiles_config_by_profile_index`
- `read_profile_file`
- `save_profile_file`
- `reorder_profile`
- `get_next_update_time`

Profile 的 `<id-or-name>` 解析应在 service 层统一处理，避免 CLI 和 GUI 产生不同语义。

### 7.5 Verge Setting

```text
clash-verge cli setting get [key]
clash-verge cli setting set <key> <value>
clash-verge cli setting patch --file verge-patch.yaml
clash-verge cli system-proxy on|off|status
clash-verge cli tun on|off|status
clash-verge cli dns save --file dns.yaml
clash-verge cli dns apply|disable|validate|show
```

映射现有能力：

- `get_verge_config`
- `patch_verge_config`
- `get_sys_proxy`
- `get_auto_proxy`
- `save_dns_config`
- `apply_dns_config`
- `check_dns_config_exists`
- `get_dns_config_content`
- `validate_dns_config`

`setting set` 需要支持类型推断和 schema 校验：

- `bool`：`true/false/on/off/1/0`
- `u16/u64/i16/i32`
- `string`
- `json`
- `yaml`

对 `IVerge` 中不存在的 key 必须返回 `invalid_argument`。

### 7.6 Service / System

```text
clash-verge cli service status
clash-verge cli service install --yes
clash-verge cli service uninstall --yes
clash-verge cli service reinstall --yes
clash-verge cli service repair --yes
clash-verge cli uwp apply
clash-verge cli network interfaces
clash-verge cli network hostname
```

映射现有能力：

- `is_service_available`
- `install_service`
- `uninstall_service`
- `reinstall_service`
- `repair_service`
- `invoke_uwp_tool`
- `get_network_interfaces`
- `get_network_interfaces_info`
- `get_system_hostname`
- `is_port_in_use`

这些命令在 Windows 上经常涉及管理员权限。CLI 必须清晰返回：

- 当前是否管理员。
- 是否需要提权。
- 是否已经触发 UAC。
- 操作是否被取消。

### 7.7 Backup / WebDAV

```text
clash-verge cli backup create
clash-verge cli backup list [--json]
clash-verge cli backup delete <filename> --yes
clash-verge cli backup restore <filename> --yes
clash-verge cli backup import <path>
clash-verge cli backup export <filename> <destination>

clash-verge cli webdav config set --url URL --username USER --password-env ENV
clash-verge cli webdav backup
clash-verge cli webdav list [--json]
clash-verge cli webdav delete <filename> --yes
clash-verge cli webdav restore <filename> --yes
```

映射现有能力：

- `create_local_backup`
- `list_local_backup`
- `delete_local_backup`
- `restore_local_backup`
- `import_local_backup`
- `export_local_backup`
- `save_webdav_config`
- `create_webdav_backup`
- `list_webdav_backup`
- `delete_webdav_backup`
- `restore_webdav_backup`

密码输入要求：

- 支持交互式隐藏输入。
- 支持 `--password-env ENV_NAME`。
- 不推荐支持明文 `--password`，如支持必须输出安全警告。

## 8. 输出与退出码

默认输出人类可读：

```text
App: running
Core: running
Mode: rule
System Proxy: enabled
TUN: disabled
Profile: MyProfile
```

`--json` 输出稳定 JSON：

```json
{
  "app": "running",
  "core": "running",
  "mode": "rule",
  "system_proxy": true,
  "tun": false,
  "profile": {
    "uid": "abc",
    "name": "MyProfile"
  }
}
```

退出码：

```text
0  success
1  generic_error
2  invalid_argument
3  unavailable
4  permission_denied
5  authentication_failed
6  conflict_or_busy
7  validation_failed
8  cancelled
9  not_found
```

## 9. 状态同步和并发控制

必须避免 CLI 和 GUI 同时写配置。

同步策略：

- GUI 已运行时，CLI 只通过 IPC 请求 GUI 进程执行写操作。
- Headless 写操作必须获取跨进程文件锁。
- 写入配置统一走 `Config::*().edit_draft -> apply/discard -> save_file/save_config`。
- 涉及运行态副作用时统一触发：
  - `CoreManager` 启停或刷新。
  - `handle::Handle::refresh_clash()`。
  - `handle::Handle::refresh_verge()`。
  - `tray::Tray::global().update_*()`。
  - `sysopt::Sysopt::global().update_sysproxy()`。
  - `hotkey::Hotkey::global().update()`。

建议新增文件锁：

```text
<app-home>/clash-verge.lock
```

锁粒度：

- 配置写入：独占锁。
- 只读状态：无锁或共享锁。
- 备份恢复：独占锁，并暂停自动备份/自动刷新。

## 10. Headless 模式边界

允许 headless：

- 读取 app 目录和配置文件。
- 读取/修改 `verge.yaml`、`profiles.yaml`、`config.yaml` 的离线配置。
- 导入/导出本地备份。
- 读取本地备份列表。
- 验证 YAML 和脚本。
- 输出将于下次 GUI 启动生效的设置。

不建议 headless：

- 启停 GUI 管理的 core 进程。
- 修改系统代理。
- 开启/关闭 TUN。
- 安装/卸载服务。
- 托盘、窗口、全局热键、轻量模式。
- 依赖 `APP_HANDLE` 或 `tauri-plugin-mihomo` 的实时状态。

这些命令在 GUI 未运行时返回：

```text
GUI backend is not running; this command requires the running app.
```

可以提供：

```text
clash-verge cli app launch
```

但启动 GUI 后仍通过 IPC 等待 ready 再执行。

## 11. 安全策略

危险操作必须显式确认或提供 `--yes`：

- `service install|uninstall|reinstall|repair`
- `tun on`
- `system-proxy on|off`
- `profile delete`
- `backup restore|delete`
- `webdav restore|delete`
- `app quit|restart`

支持 dry-run：

```text
clash-verge cli setting set enable_tun_mode true --dry-run
```

dry-run 输出：

- 将调用的 service 方法。
- 将修改的配置 key。
- 需要的副作用，例如 restart core、update tray、update system proxy。
- 是否需要管理员权限。

认证要求：

- IPC token 默认只对当前用户可读。
- HTTP fallback 必须绑定 `127.0.0.1`，并要求 token。
- 所有 request 记录审计日志，但敏感字段打码。
- WebDAV 密码、订阅 URL secret、controller secret 不输出到日志和 JSON。

## 12. 实施阶段

### 阶段 1：基础 CLI 和只读能力

- 引入 CLI 参数解析，建议使用 `clap`。
- 实现 `clash-verge cli status --json`。
- 实现 `app dir`、`core info`、`setting get`、`profile list`。
- 建立统一 `CliResult`、输出格式和退出码。
- 不启动 GUI 时可执行只读 headless。

验收：

- GUI 运行和未运行时，只读命令均能正常输出。
- JSON 输出可被 PowerShell、jq、Node 脚本稳定解析。

### 阶段 2：IPC Bridge

- 新增 `ipc::server`，GUI 启动后监听本机 IPC。
- 新增 `cli::client`，CLI 自动连接 GUI。
- 实现 token 认证和 endpoint 文件。
- 将 `status`、`core restart`、`setting set` 通过 IPC 执行。

验收：

- CLI 修改系统代理后 GUI 立即刷新。
- CLI 重启 core 后 GUI 首页和托盘状态一致。
- 无 token 或 token 错误时拒绝执行。

### 阶段 3：Service 抽象

- 将 `cmd::*` 中非 Tauri 专属逻辑逐步迁移到 `service::*`。
- `cmd::*` 和 `cli::*` 共同调用 service。
- 先迁移 `verge`、`clash`、`profile`、`backup`。

验收：

- GUI 原有功能行为不变。
- CLI 和 GUI 操作同一功能产生相同配置和副作用。

### 阶段 4：完整命令覆盖

- 补齐 profile、proxy、connection、dns、service、backup、webdav、network、lightweight。
- 增加 `--dry-run`、`--yes`、`--json`、`--wait`。
- 增加 shell completion。

验收：

- GUI 主要设置页、Profile 页、代理页、日志/连接页、备份页均有 CLI 等价命令。
- README 和 `clash-verge cli help` 能覆盖常用脚本场景。

### 阶段 5：测试和发布

- 增加单元测试、集成测试和 Windows 权限测试。
- 在安装包中加入 CLI 入口或 PATH 提示。
- 编写迁移说明和安全说明。

## 13. 测试策略

单元测试：

- 参数解析。
- key/value 类型转换。
- JSON/YAML 输出。
- 错误码映射。
- token 生成和校验。

集成测试：

- CLI -> IPC -> service -> config。
- `setting set enable_system_proxy true` 后 GUI 状态刷新。
- `profile switch` 后 runtime config 刷新。
- `core mode global` 后 Mihomo 状态一致。
- `backup create/list/restore` 完整流程。

平台测试：

- Windows 普通用户和管理员。
- Windows 服务安装/卸载/UAC。
- Windows Named Pipe ACL。
- macOS/Linux Unix Socket 权限。
- portable 模式路径。

回归测试：

- GUI 操作后 CLI `status` 一致。
- CLI 操作后 GUI 页面、托盘、运行态一致。
- GUI 未运行时 headless 命令不会破坏配置。
- 异常中断后 lock 能释放或自动恢复。

## 14. 关键风险与应对

风险：Tauri command 中混入大量 UI/AppHandle 依赖，CLI 无法直接复用。

应对：先走 GUI IPC 主路径；再逐步把纯业务逻辑抽到 service，保留 GUI 专属通知在 service 的 side-effect adapter 中。

风险：直接复用现有 embedded server 会扩大攻击面。

应对：新增独立 IPC bridge，强制 token，本机访问，敏感操作确认。

风险：CLI 与 GUI 并发写配置导致 YAML 损坏或状态不同步。

应对：运行态写操作只交给 GUI 进程；headless 写操作加文件锁。

风险：不同平台的系统代理、TUN、服务权限差异大。

应对：service 层返回结构化 capability 和 permission 信息，CLI 根据平台显示明确错误和提权建议。

风险：命令覆盖面过大导致首次实现周期长。

应对：按阶段交付，先完成只读和高频设置，再覆盖完整 GUI。

## 15. 推荐最小可行版本

MVP 命令：

```text
clash-verge cli status [--json]
clash-verge cli app dir
clash-verge cli core info
clash-verge cli core restart
clash-verge cli core mode rule|global|direct
clash-verge cli setting get [key]
clash-verge cli setting set enable_system_proxy true|false
clash-verge cli setting set enable_tun_mode true|false --yes
clash-verge cli profile list [--json]
clash-verge cli profile switch <id-or-name>
clash-verge cli profile update <id-or-name>
clash-verge cli proxy groups [--json]
clash-verge cli proxy select <group> <node>
```

MVP 技术闭环：

- CLI 参数解析。
- GUI IPC server/client。
- token 认证。
- `status` 聚合。
- `patch_verge`、`patch_clash_mode`、`restart_core`、`patch_profiles_config_by_profile_index` 的 IPC 调用。
- GUI 和 CLI 状态同步验证。

## 16. 结论

要实现“用命令行完整控制 Clash Verge GUI 功能”，正确方向不是直接改 YAML，也不是只依赖 Mihomo external-controller，而是在 Clash Verge Rev 内新增 CLI Bridge。GUI 已运行时，CLI 通过本地 IPC 调用正在运行的后端；GUI 未运行时，CLI 只执行安全的 headless 操作。业务逻辑从 Tauri command 入口下沉到共享 service 层，由 GUI invoke 和 CLI 共同复用。

该方案可以最大程度保持 GUI 与 CLI 行为一致，避免状态竞争，并为脚本化、自动化、远程运维跳板、本地快捷命令和后续测试提供稳定基础。
