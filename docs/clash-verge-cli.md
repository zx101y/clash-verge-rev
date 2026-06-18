# Clash Verge Rev CLI 完整使用手册

本文档说明 `clash-verge-cli` 当前已经实现的全部命令、参数、输出方式和使用限制。

## 1. 工作方式

CLI 有两种工作模式：

1. GUI 已运行：CLI 通过绑定在 `127.0.0.1` 的认证 IPC Bridge 请求 Clash Verge Rev 后端执行操作。核心控制、系统设置、Profile 更新、代理选择、备份等副作用与 GUI 共用同一套 Rust 业务逻辑。
2. GUI 未运行：仅部分只读命令直接读取本地配置文件，不会启动 GUI 或 Mihomo。

IPC 使用应用数据目录中的随机令牌 `.cli-token` 认证。令牌不应复制给其他用户或写入脚本日志。

## 2. 安装位置与启动

Windows 安装包会将 `clash-verge-cli.exe` 安装到 Clash Verge Rev 主程序目录。可在该目录直接运行：

```powershell
.\clash-verge-cli.exe status
```

将安装目录加入 `PATH` 后，可以省略路径和 `.exe`：

```powershell
clash-verge-cli status
```

查看内置帮助：

```text
clash-verge-cli help
clash-verge-cli --help
clash-verge-cli -h
```

## 3. 通用语法

```text
clash-verge-cli [--json] [--yes|-y] <command> [arguments]
```

全局选项可以放在命令前面或后面：

| 选项 | 说明 |
| --- | --- |
| `--json` | 输出适合脚本处理的 JSON |
| `--yes`、`-y` | 确认危险、破坏性或高权限操作 |
| `--help`、`-h` | 显示帮助 |

包含空格的 Profile 名称、代理组、节点名称和路径必须使用引号：

```powershell
clash-verge-cli profile switch "Work Profile"
clash-verge-cli proxy select "Proxy Group" "Hong Kong 01"
clash-verge-cli backup export backup.zip "D:\Clash Backups\backup.zip"
```

## 4. GUI 运行要求

以下命令在 GUI 未运行时仍可工作：

```text
status
app dir
core info
setting get [key]
profile list
help
```

其中 `status` 会优先连接 GUI；连接不到 GUI 时读取本地 `verge.yaml`、`config.yaml` 和 `profiles.yaml`。

其他命令需要 Clash Verge Rev GUI 正在运行，否则返回：

```text
error: GUI CLI bridge is not reachable; start Clash Verge first
```

## 5. 状态与目录

### 5.1 查看整体状态

```text
clash-verge-cli status
clash-verge-cli --json status
```

GUI 运行时显示：

- 应用运行状态。
- Core 运行模式。
- Mixed、SOCKS、HTTP 和控制器信息。
- 系统代理、TUN、外部控制器状态。
- 当前 Profile UID 和名称。

GUI 未运行时显示本地配置快照，并明确标记应用未检测到。

PowerShell 获取当前 Profile：

```powershell
$status = clash-verge-cli --json status | ConvertFrom-Json
$status.profile.name
```

### 5.2 查看应用数据目录

```text
clash-verge-cli app dir
clash-verge-cli --json app dir
```

JSON 示例：

```json
{
  "app_home": "C:\\Users\\User\\AppData\\Roaming\\io.github.clash-verge-rev.clash-verge-rev"
}
```

便携模式下，CLI 会优先识别程序目录中的 `.config/PORTABLE`。

## 6. Core 控制

### 6.1 查看 Core 配置

```text
clash-verge-cli core info
clash-verge-cli --json core info
```

显示 Mixed Port、SOCKS Port、HTTP Port、模式、External Controller 地址及其启用状态。该命令读取本地配置，GUI 可以不运行。

### 6.2 启动 Core

```text
clash-verge-cli core start
```

要求 GUI 正在运行。成功后通知 GUI 刷新 Core 状态。

### 6.3 停止 Core

```text
clash-verge-cli core stop
```

要求 GUI 正在运行。停止前会保存 Profile 配置，成功后刷新 GUI 状态。

### 6.4 重启 Core

```text
clash-verge-cli core restart
```

要求 GUI 正在运行。适合在修改配置或排查运行状态后使用。

### 6.5 修改运行模式

```text
clash-verge-cli core mode rule
clash-verge-cli core mode global
clash-verge-cli core mode direct
```

仅接受：

| 值 | 含义 |
| --- | --- |
| `rule` | 规则模式 |
| `global` | 全局代理模式 |
| `direct` | 全局直连模式 |

模式会同步写入配置，并通过 GUI 使用的 Mihomo client 更新运行态。

## 7. Verge 设置

### 7.1 查看所有设置

```text
clash-verge-cli setting get
clash-verge-cli --json setting get
```

读取 `verge.yaml`。密码、令牌和 secret 类字段会显示为 `<redacted>`。

### 7.2 查看单个设置

```text
clash-verge-cli setting get enable_system_proxy
clash-verge-cli setting get hotkeys.global
```

读取支持点分路径；不存在的键返回退出码 `9`。

### 7.3 修改设置

```text
clash-verge-cli setting set <key> <value>
```

当前只允许修改 `IVerge` 的顶层键，不支持用点分路径修改嵌套字段。

示例：

```text
clash-verge-cli setting set enable_system_proxy true
clash-verge-cli setting set enable_tun_mode false
clash-verge-cli setting set theme_mode dark
clash-verge-cli setting set verge_mixed_port 7897
```

值按 YAML 标量解析：

| 输入 | 解析类型 |
| --- | --- |
| `true`、`false` | 布尔值 |
| `7897` | 数字 |
| `null` | 空值 |
| `dark` | 字符串 |

需要强制保留为字符串时，可以在 shell 参数中包含 YAML 引号：

```powershell
clash-verge-cli setting set theme_mode '"dark"'
```

修改操作要求 GUI 运行。系统代理、TUN、热键、托盘、日志等设置仍由原 GUI 后端处理相应副作用。

## 8. Profile 管理

`<id-or-name>` 可以是 Profile UID，也可以是完整名称。名称包含空格时必须加引号。

### 8.1 列出 Profile

```text
clash-verge-cli profile list
clash-verge-cli --json profile list
```

人类可读输出使用 `*` 标记当前 Profile，并显示名称、类型和 UID。该命令可在 GUI 未运行时读取本地配置。

### 8.2 切换 Profile

```text
clash-verge-cli profile switch <id-or-name>
```

示例：

```text
clash-verge-cli profile switch abc123
clash-verge-cli profile switch "Office Subscription"
```

切换过程执行运行时配置验证；验证失败时恢复原 Profile，不会保留无效切换。

### 8.3 更新远程 Profile

```text
clash-verge-cli profile update <id-or-name>
```

使用 GUI 相同的订阅更新逻辑下载、验证并应用远程 Profile。

### 8.4 删除 Profile

```text
clash-verge-cli profile delete <id-or-name> --yes
```

这是危险操作，缺少 `--yes` 时 CLI 会在发送 IPC 请求前拒绝执行。

删除当前 Profile 时，后端会按 GUI 原有逻辑更新运行时配置、托盘和定时器。

### 8.5 读取 Profile 文件

```text
clash-verge-cli profile read <id-or-name>
clash-verge-cli --json profile read <id-or-name>
```

JSON 输出：

```json
{
  "content": "mixed-port: 7897\n..."
}
```

## 9. 代理组与节点

### 9.1 获取代理组和节点

```text
clash-verge-cli proxy groups
clash-verge-cli --json proxy groups
```

返回 Mihomo 的实时代理数据，包括：

- 代理组名称和类型。
- 当前选中节点 `now`。
- 可选节点列表 `all`。
- 节点存活状态和延迟历史。

PowerShell 查看 GLOBAL 当前节点：

```powershell
$data = clash-verge-cli --json proxy groups | ConvertFrom-Json
$data.proxies.GLOBAL.now
```

### 9.2 选择节点

```text
clash-verge-cli proxy select <group> <node>
```

示例：

```text
clash-verge-cli proxy select GLOBAL DIRECT
clash-verge-cli proxy select "Proxy Group" "Hong Kong 01"
```

成功后会通知 GUI 刷新代理状态，并更新托盘菜单。

## 10. 连接管理

### 10.1 列出活动连接

```text
clash-verge-cli connection list
clash-verge-cli --json connection list
```

返回 Mihomo 实时连接数据、上传/下载总量和内存信息。没有活动连接时，`connections` 可能为 `null`。

### 10.2 关闭指定连接

```text
clash-verge-cli connection close <connection-id> --yes
```

连接 ID 可从 `connection list --json` 中取得。

### 10.3 关闭所有连接

```text
clash-verge-cli connection close --all --yes
```

关闭连接属于破坏性操作，必须提供 `--yes` 或 `-y`。

## 11. 本地备份

### 11.1 创建备份

```text
clash-verge-cli backup create
```

使用 GUI 相同的本地备份逻辑创建 ZIP 备份。

### 11.2 列出备份

```text
clash-verge-cli backup list
clash-verge-cli --json backup list
```

每项包含文件名、路径、大小和修改时间。

### 11.3 删除备份

```text
clash-verge-cli backup delete <filename> --yes
```

`filename` 应使用 `backup list` 返回的文件名，而不是任意路径。

### 11.4 恢复备份

```text
clash-verge-cli backup restore <filename> --yes
```

恢复会覆盖当前应用数据，必须显式确认。

### 11.5 导入备份

```text
clash-verge-cli backup import <source-path>
```

Windows 示例：

```powershell
clash-verge-cli backup import "D:\Backups\clash-verge.zip"
```

文件会导入应用的本地备份目录。

### 11.6 导出备份

```text
clash-verge-cli backup export <filename> <destination>
```

示例：

```powershell
clash-verge-cli backup export linux-backup.zip "D:\Backups\clash-verge.zip"
```

## 12. WebDAV 备份

### 12.1 配置 WebDAV

```text
clash-verge-cli webdav config <url> <username> <password-env>
```

第三个参数是保存密码的环境变量名称，不是明文密码。这样可以避免密码出现在命令历史和进程参数中。

PowerShell：

```powershell
$env:CVR_DAV_PASSWORD = "your-password"
clash-verge-cli webdav config "https://dav.example.com/clash" "user" CVR_DAV_PASSWORD
Remove-Item Env:CVR_DAV_PASSWORD
```

CMD：

```bat
set CVR_DAV_PASSWORD=your-password
clash-verge-cli webdav config https://dav.example.com/clash user CVR_DAV_PASSWORD
set CVR_DAV_PASSWORD=
```

环境变量不存在时返回退出码 `2`。

### 12.2 创建并上传备份

```text
clash-verge-cli webdav backup
```

### 12.3 列出远程备份

```text
clash-verge-cli webdav list
clash-verge-cli --json webdav list
```

### 12.4 删除远程备份

```text
clash-verge-cli webdav delete <filename> --yes
```

### 12.5 恢复远程备份

```text
clash-verge-cli webdav restore <filename> --yes
```

删除和恢复均要求 `--yes`。

## 13. DNS 配置

DNS 命令操作 GUI 保存的独立 DNS 配置文件。

### 13.1 查看 DNS 配置

```text
clash-verge-cli dns show
clash-verge-cli --json dns show
```

配置文件不存在时命令失败。JSON 输出的 YAML 内容位于 `content` 字段。

### 13.2 验证 DNS 配置

```text
clash-verge-cli dns validate
clash-verge-cli --json dns validate
```

返回 Core 配置验证结果。

### 13.3 应用 DNS 配置

```text
clash-verge-cli dns apply
```

将保存的 DNS 配置合并到运行时配置，并要求 Core 接受新配置。

### 13.4 禁用独立 DNS 配置

```text
clash-verge-cli dns disable
```

重新生成运行时配置，不再加载独立 DNS 配置文件。

## 14. 系统服务

Windows 上服务操作通常需要管理员权限或触发 UAC。

### 14.1 查看服务可用性

```text
clash-verge-cli service status
clash-verge-cli --json service status
```

JSON 示例：

```json
{
  "available": true
}
```

### 14.2 安装服务

```text
clash-verge-cli service install --yes
```

### 14.3 卸载服务

```text
clash-verge-cli service uninstall --yes
```

### 14.4 重装服务

```text
clash-verge-cli service reinstall --yes
```

### 14.5 修复服务

```text
clash-verge-cli service repair --yes
```

所有服务修改命令都必须提供 `--yes`。

## 15. 网络信息

### 15.1 获取主机名

```text
clash-verge-cli network hostname
```

### 15.2 获取网络接口

```text
clash-verge-cli network interfaces
clash-verge-cli --json network interfaces
```

JSON 示例：

```json
[
  "Ethernet",
  "Wi-Fi"
]
```

## 16. 轻量模式

### 16.1 查看状态

```text
clash-verge-cli lightweight status
clash-verge-cli --json lightweight status
```

当前实现返回 Core Manager 的运行模式，例如：

```json
{
  "running_mode": "Sidecar"
}
```

该字段不是单独的布尔值。

### 16.2 进入轻量模式

```text
clash-verge-cli lightweight on
```

### 16.3 退出轻量模式

```text
clash-verge-cli lightweight off
```

## 17. JSON 输出与脚本

建议自动化脚本始终使用 `--json`，并检查进程退出码。

PowerShell 示例：

```powershell
$result = clash-verge-cli --json status | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw "Failed to query Clash Verge"
}

if (-not $result.verge.system_proxy) {
    clash-verge-cli setting set enable_system_proxy true
}
```

切换 Profile 并验证：

```powershell
clash-verge-cli profile switch "Office Subscription"
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$status = clash-verge-cli --json status | ConvertFrom-Json
$status.profile
```

批量关闭连接：

```powershell
$connections = clash-verge-cli --json connection list | ConvertFrom-Json
if ($connections.connections) {
    clash-verge-cli connection close --all --yes
}
```

## 18. 退出码

| 退出码 | 名称 | 含义 |
| --- | --- | --- |
| `0` | success | 命令成功 |
| `1` | generic_error | 后端操作或未知错误 |
| `2` | invalid_argument | 命令、参数、值或确认选项无效 |
| `3` | unavailable | GUI Bridge、配置目录或运行时不可用 |
| `5` | authentication_failed | IPC 令牌认证失败 |
| `9` | not_found | 设置键等目标不存在 |

CLI 错误写入标准错误流：

```text
error: profile delete requires --yes
```

PowerShell 可通过 `$LASTEXITCODE` 获取退出码：

```powershell
clash-verge-cli core restart
Write-Host "Exit code: $LASTEXITCODE"
```

## 19. 安全说明

- IPC 仅监听 `127.0.0.1`，不会监听局域网地址。
- `.cli-token` 至少包含 256 bit 随机数据；Unix 平台权限设为 `0600`。
- 不要提交、共享或打印 `.cli-token`。
- `setting get` 会遮蔽名称包含 `password`、`secret` 或 `token` 的字段。
- WebDAV 密码应通过环境变量传递。
- 删除、恢复、服务操作和关闭连接需要 `--yes`。
- CLI 操作运行态时必须经 GUI Bridge，避免 GUI 与另一个进程并发写配置。

## 20. 常见问题

### GUI 已启动但提示 Bridge 不可用

确认 CLI 与 GUI 属于同一个安装版本和用户，并检查是否有其他程序占用了单例端口。完全退出 Clash Verge Rev 后重新启动。

### 修改设置提示只支持顶层键

当前 `setting set` 不接受 `a.b.c` 形式的嵌套路径。请使用 `setting get` 确认顶层字段名称。

### Profile 名称找不到

名称必须完整匹配。可以先运行：

```text
clash-verge-cli profile list
```

然后使用输出中的 UID，避免重名或空格问题。

### DNS show 提示文件不存在

需要先在 GUI 的 DNS 设置中保存独立 DNS 配置文件；CLI 当前没有 `dns save` 命令。

### Windows 服务操作失败

请从管理员 PowerShell 启动命令，或者确认 UAC 请求未被取消。

### 为什么某些命令在 GUI 未运行时失败

Core、系统代理、TUN、代理节点、连接、备份恢复和服务操作依赖 GUI 进程中的状态、插件或副作用管理，不能通过直接修改 YAML 安全完成。
