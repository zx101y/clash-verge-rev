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

### 9.3 查询所有节点及所属组

```text
clash-verge-cli proxy nodes
clash-verge-cli --json proxy nodes
```

默认输出只包含组名称和节点名称：

```text
GROUP          NODE
Auto Select    Hong Kong 01
Auto Select    Singapore 01
Proxy          Hong Kong 01
Proxy          Japan 01
```

同一节点属于多个组时会显示多行。嵌套代理组不会被当作普通节点列出，但其内部的叶子节点会以实际所属组显示。

JSON 示例：

```json
[
  {
    "group": "Proxy",
    "node": "Hong Kong 01"
  }
]
```

### 9.4 测试所有节点延迟

```text
clash-verge-cli proxy test
clash-verge-cli --json proxy test
```

CLI 对所有唯一节点执行实时延迟测试，再按延迟从小到大排列：

```text
GROUP          NODE            DELAY
Proxy          Hong Kong 01    42 ms
Auto Select    Singapore 01    68 ms
Proxy          Offline Node    0 ms
```

测速规则：

- 测试地址为 `https://www.gstatic.com/generate_204`。
- 单节点超时为 5000 毫秒。
- 最多并发测试 16 个唯一节点。
- 同一节点出现在多个组中时只测速一次，再为每个所属组输出一行。
- 延迟为 `0` 表示超时或测速失败，并排在所有有效延迟之后。

节点较多时命令可能需要数秒完成。

### 9.5 查询当前选择的节点

```text
clash-verge-cli proxy current
clash-verge-cli --json proxy current
```

列出每个代理组当前选择的节点，并对当前节点进行实时延迟测试：

```text
GROUP          NODE            DELAY
GLOBAL         DIRECT          1 ms
Proxy          Hong Kong 01    42 ms
Auto Select    Singapore 01    68 ms
```

JSON 示例：

```json
[
  {
    "group": "Proxy",
    "node": "Hong Kong 01",
    "delay": 42
  }
]
```

一个配置通常包含多个代理组，因此该命令可能返回多行，而不是单一节点。

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

## 附录 A：修改版 Windows GUI 的构建、部署与启动

本附录用于从当前仓库源码构建包含 CLI Bridge 的 Windows GUI，并将 GUI 与 `clash-verge-cli.exe` 一起部署到 Windows。

### A.1 三类目录的区别

构建和排查问题时，必须区分以下目录：

| 目录 | 用途 | 典型路径 |
| --- | --- | --- |
| 源码目录 | Git 仓库、依赖和构建输出 | `C:\src\clash-verge-rev` |
| 程序安装目录 | GUI、CLI 和 sidecar 可执行文件 | `C:\Program Files\Clash Verge` |
| 应用数据目录 | YAML 配置、Profile、日志和 `.cli-token` | `%APPDATA%\io.github.clash-verge-rev.clash-verge-rev` |

`clash-verge-cli app dir` 返回的是应用数据目录，不是 GUI 程序目录。因此该目录下没有 `clash-verge.exe` 是正常现象。

### A.2 为什么必须重新构建 GUI

CLI Bridge 运行在修改后的 GUI 后端中。官方原版或修改前的 Clash Verge Rev 不包含以下内容：

- `/cli/v1/invoke` 本地控制接口。
- `.cli-token` 生成和认证。
- CLI 请求到共享 Service 的分发。

因此，只复制新编译的 `clash-verge-cli.exe`，不能控制旧版 GUI。GUI 和 CLI 必须来自包含这些提交的同一份源码：

```text
2e02f3ec feat(cli): add authenticated GUI IPC bridge
cf94580e refactor(cli): share GUI control services
8c14095c feat(cli): expand GUI control commands
20c0a872 build(cli): bundle Windows command-line controller
```

### A.3 Windows 构建环境

推荐使用 Windows 10/11 x64，在 PowerShell 中构建。

安装以下工具：

1. Git for Windows。
2. Node.js 24.x。项目发布流水线使用 Node.js `24.16.0`。
3. pnpm `11.3.0`。
4. Rust `1.95.0`，默认 MSVC toolchain。
5. Visual Studio 2022 Build Tools。
6. Microsoft Edge WebView2 Runtime。

Visual Studio Installer 中至少选择：

- `Desktop development with C++`。
- MSVC v143 x64/x86 build tools。
- Windows 10 或 Windows 11 SDK。

安装 Rust：

```powershell
winget install Rustlang.Rustup
rustup toolchain install 1.95.0
rustup default 1.95.0
rustup target add x86_64-pc-windows-msvc
```

安装 Node.js 和 pnpm：

```powershell
winget install OpenJS.NodeJS
corepack enable
corepack prepare pnpm@11.3.0 --activate
```

验证环境：

```powershell
git --version
node --version
pnpm --version
rustc --version
cargo --version
```

### A.4 获取并确认源码

进入当前修改版仓库：

```powershell
cd C:\src\clash-verge-rev
git status
git log -6 --oneline
```

日志中应至少能看到前述 CLI Bridge、Service、命令覆盖和打包提交。

如果源码位于 WSL 文件系统，建议复制或克隆到 Windows 本地 NTFS 目录后再执行 MSVC 构建。Windows 工具链直接构建 `\\wsl$` 路径可能遇到路径、权限或文件监听问题。

### A.5 安装前端依赖

```powershell
pnpm install --frozen-lockfile
```

若锁文件与 `package.json` 正在开发中且确实不同步，可临时使用：

```powershell
pnpm install
```

正式构建推荐保持 `--frozen-lockfile`。

### A.6 准备 Windows sidecar 和资源

执行：

```powershell
pnpm prebuild x86_64-pc-windows-msvc
```

该步骤会：

- 用 release 配置构建 `clash-verge-cli.exe`。
- 将 CLI 复制为 Tauri sidecar 命名：
  `src-tauri\sidecar\clash-verge-cli-x86_64-pc-windows-msvc.exe`。
- 下载或复用 Windows x64 Mihomo sidecar。
- 下载或复用 Clash Verge Service、GeoIP、GeoSite、MMDB 和 UWP 工具。

确认 CLI sidecar：

```powershell
Test-Path .\src-tauri\sidecar\clash-verge-cli-x86_64-pc-windows-msvc.exe
```

预期输出：

```text
True
```

### A.7 构建无发布签名的本地安装包

仓库配置了 updater 公钥。正式发布流水线通过私钥生成 updater 签名，但本地通常没有 `TAURI_SIGNING_PRIVATE_KEY`。

本地测试构建应临时关闭 updater artifact：

```powershell
pnpm tauri build `
  --target x86_64-pc-windows-msvc `
  --bundles nsis `
  --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

该临时配置只关闭 updater 签名产物，不会关闭 CLI Bridge、CLI sidecar 或 NSIS 安装包。

构建过程会自动执行前端 TypeScript 检查和 Vite production build，然后编译 GUI 后端并生成 NSIS。

主要产物：

```text
target\x86_64-pc-windows-msvc\release\clash-verge.exe
target\x86_64-pc-windows-msvc\release\clash-verge-cli.exe
target\x86_64-pc-windows-msvc\release\bundle\nsis\*-setup.exe
```

CLI 的 Tauri sidecar 源文件位于：

```text
src-tauri\sidecar\clash-verge-cli-x86_64-pc-windows-msvc.exe
```

实际文件名可能因 Tauri 版本和产品版本号略有差异，可用以下命令查找：

```powershell
Get-ChildItem .\target\x86_64-pc-windows-msvc\release\bundle\nsis\*-setup.exe
Get-ChildItem .\target\x86_64-pc-windows-msvc\release\clash-verge*.exe
```

### A.8 构建前验证

建议在安装前执行：

```powershell
cargo test -p clash-verge-cli-protocol -p clash-verge-cli
cargo test -p clash-verge --lib
cargo clippy -p clash-verge-cli-protocol -p clash-verge-cli -p clash-verge --lib -- -D warnings
pnpm typecheck
```

再检查 Windows CLI 目标：

```powershell
cargo check -p clash-verge-cli --target x86_64-pc-windows-msvc
```

### A.9 部署前备份

修改版使用与正式版相同的应用标识和应用数据目录，会读取现有配置。安装前建议创建备份。

在原 GUI 中创建本地备份，或手工复制：

```powershell
$appData = Join-Path $env:APPDATA 'io.github.clash-verge-rev.clash-verge-rev'
Copy-Item $appData "$appData.backup-$(Get-Date -Format yyyyMMdd-HHmmss)" -Recurse
```

如果目录不存在，说明当前 Windows 用户尚未运行过该应用。

### A.10 完全退出旧版 GUI

从系统托盘菜单退出 Clash Verge Rev。关闭窗口不一定会退出后台进程。

确认进程：

```powershell
Get-Process clash-verge -ErrorAction SilentlyContinue
```

正常情况下不应返回进程。如果托盘退出无效，可在确认配置已备份后执行：

```powershell
Stop-Process -Name clash-verge -Force
```

不要同时运行官方旧版和修改版，它们使用相同的单例端口和应用数据目录。

### A.11 使用 NSIS 安装包部署

推荐使用安装包，而不是单独复制 `clash-verge.exe`。GUI 运行还依赖 Mihomo、Service、资源文件和 CLI sidecar。

找到并启动安装包：

```powershell
$installer = Get-ChildItem .\target\x86_64-pc-windows-msvc\release\bundle\nsis\*-setup.exe |
  Select-Object -First 1
Start-Process $installer.FullName -Verb RunAs -Wait
```

本地构建通常没有 Authenticode 代码签名，Windows SmartScreen 可能显示“未知发布者”。仅应安装自己从可信源码构建的产物；正式分发应使用受控的 Windows 代码签名流程。

安装配置为 per-machine，通常安装到：

```text
C:\Program Files\Clash Verge
```

如果系统或安装选项使用了其他位置，可在启动后查询实际进程路径：

```powershell
(Get-Process clash-verge).Path
```

### A.12 启动修改版 GUI

可以通过开始菜单中的 Clash Verge 快捷方式启动，也可以直接运行：

```powershell
Start-Process "$env:ProgramFiles\Clash Verge\clash-verge.exe"
```

如果安装目录不同：

```powershell
Start-Process "D:\Apps\Clash Verge\clash-verge.exe"
```

等待 GUI 完成初始化。首次启动可能需要初始化配置、启动 Core、创建托盘和生成认证令牌。

确认运行的是修改版：

```powershell
$process = Get-Process clash-verge -ErrorAction Stop
$process.Path
```

程序目录中应同时存在：

```text
clash-verge.exe
clash-verge-cli.exe
verge-mihomo.exe
verge-mihomo-alpha.exe
```

具体服务文件名可能随版本变化。

### A.13 验证 CLI Bridge

先定位 CLI：

```powershell
$guiPath = (Get-Process clash-verge -ErrorAction Stop).Path
$installDir = Split-Path $guiPath
$cli = Join-Path $installDir 'clash-verge-cli.exe'
Test-Path $cli
```

检查应用数据目录：

```powershell
& $cli app dir
```

检查令牌是否生成：

```powershell
$appData = & $cli app dir
Test-Path (Join-Path $appData '.cli-token')
```

预期输出为 `True`。无需也不应打印令牌内容。

测试 Bridge：

```powershell
& $cli --json status
& $cli setting get enable_tun_mode
& $cli setting set enable_tun_mode false
```

`status` 中的：

```json
{
  "app": {
    "running": true
  }
}
```

表示 CLI 已连接修改版 GUI，而不是使用离线配置回退。

### A.14 将 CLI 加入 PATH

仅对当前 PowerShell 会话生效：

```powershell
$installDir = Split-Path (Get-Process clash-verge -ErrorAction Stop).Path
$env:PATH = "$installDir;$env:PATH"
clash-verge-cli status
```

永久加入当前用户 PATH：

```powershell
$installDir = Split-Path (Get-Process clash-verge -ErrorAction Stop).Path
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $installDir) {
    [Environment]::SetEnvironmentVariable(
        'Path',
        ($userPath.TrimEnd(';') + ';' + $installDir),
        'User'
    )
}
```

重新打开 PowerShell 后生效。

### A.15 不安装，直接运行 release 目录

不推荐只运行单个 `clash-verge.exe`。如果需要免安装测试，应确保 GUI、CLI、Mihomo sidecar、服务和 resources 的相对布局与打包结果一致。

最稳妥的免安装方式是先生成 NSIS 安装包并安装到测试目录。直接复制单个 GUI 文件会导致 Core、服务、图标、GeoIP 或 CLI 缺失。

### A.16 开发调试模式

开发 GUI 使用 `verge-dev` feature：

```powershell
pnpm prebuild x86_64-pc-windows-msvc
pnpm dev
```

开发模式与 release 模式不同：

| 项目 | Release | `verge-dev` |
| --- | --- | --- |
| 单例/Bridge 端口 | `33331` | `11233` |
| 应用标识 | `io.github.clash-verge-rev.clash-verge-rev` | `io.github.clash-verge-rev.clash-verge-rev.dev` |
| 数据目录 | 正式目录 | 以 `.dev` 结尾的开发目录 |

普通 release CLI 默认连接端口 `33331`，不能连接 `pnpm dev` 启动的 GUI。调试开发 GUI 时，需要构建带相同 feature 的 CLI：

```powershell
cargo run -p clash-verge-cli --features verge-dev -- --json status
cargo run -p clash-verge-cli --features verge-dev -- setting set enable_tun_mode false
```

也可先构建：

```powershell
cargo build -p clash-verge-cli --features verge-dev
.\target\debug\clash-verge-cli.exe --json status
```

不要混用 release CLI 与开发 GUI。

### A.17 正式发布签名

正式发布构建不应关闭 updater artifact，而应在受控环境设置：

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

然后按项目 GitHub Actions 的 Windows MSVC 流程构建。不要把私钥写入仓库、脚本或 PowerShell 历史。

本地没有发布私钥时，出现以下错误不代表 GUI 编译失败：

```text
A public key has been found, but no private key.
```

它表示主程序和安装包可能已经生成，但 updater 签名步骤无法完成。日常本地部署应使用 A.7 中关闭 updater artifact 的命令。

### A.18 升级和回退

升级修改版：

1. 用 CLI 或 GUI 创建备份。
2. 完全退出旧 GUI。
3. 构建新的 NSIS 安装包。
4. 覆盖安装。
5. 启动 GUI 并运行 `clash-verge-cli --json status`。

回退官方版：

1. 创建并导出备份。
2. 完全退出修改版。
3. 卸载修改版。
4. 安装目标官方版本。
5. 仅在配置格式兼容时复用或恢复应用数据。

由于修改版与官方版使用相同标识，回退前必须备份 `%APPDATA%` 中的数据。

### A.19 Windows 构建与启动故障排查

#### `cl.exe`、`link.exe` 或 Windows SDK 找不到

安装 Visual Studio 2022 Build Tools 的 C++ Desktop workload，并重新打开 PowerShell。必要时使用 Developer PowerShell for VS 2022。

#### `pnpm prebuild` 下载失败

检查 GitHub 网络访问和代理设置。脚本会缓存已经下载的 Mihomo、Service 和规则资源，重试时通常不需要重新下载成功项。

#### NSIS 构建成功但最后提示缺少私钥

改用 A.7 中 `createUpdaterArtifacts: false` 的本地构建命令。

#### 安装目录没有 `clash-verge-cli.exe`

确认以下配置包含 `sidecar/clash-verge-cli`：

```text
src-tauri\tauri.conf.json
src-tauri\tauri.windows.conf.json
```

并确认在 Tauri build 前执行过：

```powershell
pnpm prebuild x86_64-pc-windows-msvc
```

#### `.cli-token` 不存在

依次检查：

1. 运行的是修改版 GUI，而不是官方旧版。
2. GUI 已完成初始化且没有立即退出。
3. GUI 和 CLI 由同一个 Windows 用户运行。
4. `clash-verge-cli app dir` 指向当前用户的正确数据目录。
5. 便携版 GUI 和 CLI 是否位于同一个便携目录。

#### `.cli-token` 存在但仍提示 Bridge 不可达

检查 release Bridge 端口：

```powershell
Get-NetTCPConnection -State Listen -LocalPort 33331 -ErrorAction SilentlyContinue
```

如果没有监听：

- GUI 可能不是修改版。
- GUI 后端初始化可能失败。
- 另一个旧进程可能占用了单例端口。

确认进程路径并完全重启：

```powershell
Get-Process clash-verge -ErrorAction SilentlyContinue |
  Select-Object Id, Path
```

#### GUI 已运行但 CLI 读取了错误目录

检查是否混用了：

- 不同 Windows 用户。
- 正式版和 `verge-dev`。
- 安装版和便携版。
- 不同目录中的多个 `clash-verge-cli.exe`。

查找实际执行的 CLI：

```powershell
Get-Command clash-verge-cli | Select-Object Source
```

推荐始终从 GUI 进程路径推导 CLI：

```powershell
$dir = Split-Path (Get-Process clash-verge -ErrorAction Stop).Path
& (Join-Path $dir 'clash-verge-cli.exe') --json status
```
