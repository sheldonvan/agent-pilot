# Agent Pilot 项目交接文档

更新时间：2026-05-23  
项目路径：`/Users/van/MyFile/desktop ai`  
当前产品名：`Agent Pilot`  
当前版本：`0.1.1`
当前 Git 分支：`main`  
当前基础备份提交：`c34c241 Backup Agent Pilot initial version`

## 1. 项目定位

Agent Pilot 是一个 macOS-first 的 AI Agent 管理桌面悬浮窗，用来集中观察本机或远程终端里的 Agent 状态。当前第一版主要解决：

- 自动发现本地终端、SSH 终端、tmux 会话里的 Agent。
- 每分钟自动刷新，终端关闭后自动清理，检测到新终端后自动加入列表。
- 对本地 Claude Code / Codex CLI / 远程 SSH 会话进行状态监测。
- 对 Codex Desktop 进行第一版监测：识别 Codex 桌面端进程，并通过 Codex 本地 SQLite 日志判断是否需要关注。
- 提供 macOS 风格 UI、窗口拖拽、红绿灯窗口按钮、设置弹窗、主题切换、备注改名、手动添加 Agent 等基础体验。

## 2. 技术栈

- 桌面壳：Tauri v2
- 后端：Rust
- 前端：原生 HTML / CSS / JavaScript，无框架
- macOS 集成：`ps`、`lsof`、`osascript`、`open`、`sqlite3`、`tmux`、`ssh`
- 本地 API：Rust 内置轻量 HTTP collector，默认监听 `127.0.0.1:8787`
- 前端 dev server：`python3 -m http.server 5174 -d web`

## 3. 关键文件

```text
.
├── PROJECT_HANDOFF.md                              # 当前交接文档
├── README.md                                      # 简版项目说明
├── AGENT_PILOT_MVP_SPEC_MACOS_V1_2_DISCOVERY.md   # 早期 MVP / Discovery 设计文档
├── package.json                                   # npm scripts，仅包装 dev/build 命令
├── dist/Agent Pilot_0.1.1_aarch64.dmg             # 当前已构建 dmg，未正式签名
├── examples/config.example.json                   # 远程扫描配置示例
├── scripts/
│   ├── agent-pilot                                # wrapper 示例
│   ├── claude-hook-example.sh                     # Claude hook 示例
│   └── codex-notify-example.sh                    # Codex notify 示例
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json                            # Tauri 应用配置
│   ├── capabilities/default.json                  # Tauri 权限能力
│   └── src/main.rs                                # 后端主逻辑
└── web/
    ├── index.html                                 # 前端结构
    ├── app.js                                     # 前端状态、渲染、交互
    ├── styles.css                                 # macOS 风格 UI
    └── assets/marvis-reference.png                # UI 参考图资源
```

## 4. 运行方式

### 4.1 桌面端 dev 运行

推荐使用独立 target 目录，避免项目内 `src-tauri/target` 过大或权限混乱：

```bash
CARGO_TARGET_DIR=/private/tmp/agent-pilot-run-target cargo tauri dev
```

Tauri dev 会自动执行：

```bash
python3 -m http.server 5174 -d web
```

桌面端窗口加载：

```text
http://127.0.0.1:5174
```

后端 API 默认监听：

```text
http://127.0.0.1:8787/api
```

### 4.2 前端静态预览

只看 UI，不启动 Rust 后端：

```bash
python3 -m http.server 5174 -d web
```

然后打开：

```text
http://127.0.0.1:5174
```

注意：如果没有 Tauri invoke 且连接不到 `127.0.0.1:8787`，`web/app.js` 会 fallback 到 mock 数据。

### 4.3 构建 dmg

```bash
cargo tauri build
```

当前已有 dmg：

```text
dist/Agent Pilot_0.1.1_aarch64.dmg
```

当前 dmg 未做 Apple Developer ID 签名和 notarization。分发给别人时，macOS 可能提示“已损坏，无法打开”。临时测试可使用：

```bash
xattr -dr com.apple.quarantine "/Applications/Agent Pilot.app"
```

长期分发需要正式签名、公证。

## 5. 当前配置位置

运行后会读写：

```text
~/.agent-pilot/config.json
```

该配置保存：

- app 监听地址与默认终端
- discovery 开关与刷新间隔
- remoteHosts
- 已管理 agents
- 用户修改后的备注名称

Codex Desktop 监测会读取：

```text
~/.codex/logs_2.sqlite
```

Codex Desktop 相关状态来自该日志库的 app-server event。

## 6. 后端结构概览

主文件：

```text
src-tauri/src/main.rs
```

核心数据模型：

- `DeskSnapshot`：前端整体状态快照。
- `DeskConfig`：持久化配置。
- `AgentItem`：已管理 Agent。
- `DiscoveredAgent`：扫描阶段发现的候选 Agent。
- `TerminalTarget`：点击“打开终端 / 定位终端 / 定位 Codex”时使用的目标信息。
- `AgentKind`：当前包含 `codex`、`codex_desktop`、`claude_code`、`other`、`unknown`。
- `AgentStatus`：当前包含 `running`、`waiting_attention`、`done`、`offline`、`error`。

核心流程：

- `main()`：创建 Tauri app，注册 commands，启动 collector 和后台 scanner。
- `start_collector()`：启动本地 HTTP collector，处理 `/api/*` 请求。
- `start_background_scanner()`：后台定时扫描，当前最小间隔归一化为约 60 秒。
- `run_scan()`：统一扫描入口，合并本地进程、本地 tmux、Codex Desktop、远程进程、远程 tmux 的扫描结果。
- `prune_inactive_discovered_agents()`：扫描源消失后自动移除不再存在的 discovered agents。
- `open_terminal_inner()`：点击前端按钮后的终端定位/打开入口。

扫描来源：

```text
local_process
local_ssh
local_ssh_tmux
local_tmux
codex_desktop
remote_process
remote_tmux
vscode_remote_ssh
vscode_remote_process
vscode_remote_tmux
```

## 7. 前端结构概览

主文件：

```text
web/app.js
web/styles.css
web/index.html
```

核心逻辑：

- `syncState()`：从 Tauri command 或 HTTP API 同步状态。
- `startAutoRefresh()`：每分钟自动刷新。
- `render()`：刷新统计、列表、候选项。
- `renderAgentCard()`：渲染每个 Agent 卡片。
- `openButtonText()`：根据 target 类型显示“定位 Codex / 定位 VS Code / 定位终端 / 查看进程 / 打开终端”等按钮文案。
- `setupWindowDragging()`：自定义拖拽区域，解决 Tauri overlay 标题栏拖拽问题。
- `applyTheme()`：主题模式切换，支持 `system`、`dark`、`light`。

UI 当前形态：

- 顶部正中显示 `Agent Pilot`。
- 顶部统计区显示在线和需关注。
- 右侧有设置按钮和添加按钮，当前是上下排布。
- Agent block 默认收起。
- “打开终端”按钮放在卡片外层摘要区，位于状态下方。
- 支持鼠标 hover 名字附近显示铅笔，点击后编辑备注，按 Enter 或 ✓ 保存。
- 设置弹窗当前支持背景主题切换：随系统、黑色、白色。

## 8. 当前已实现功能清单

### 8.1 进程与终端发现

- 本地进程扫描：识别本机正在跑的 Agent 命令。
- 本地 SSH 扫描：只要命令是 `ssh`，就放宽识别为 Remote，不再强依赖 tmux 固定搭配。
- 本地 tmux 扫描：捕获 tmux pane 输出，用于判断 Agent 状态。
- VS Code Remote-SSH 自动发现：扫描本机 VS Code `ms-vscode-remote.remote-ssh` 的 `localServer.js` 进程，解析其中的 `sshArgs` 与 data 文件，识别 `ssh-remote+...` 会话，再尝试用同一 SSH alias 做后台远程进程/tmux 扫描。
- 远程扫描：需要配置 `remoteHosts` 且 SSH 免密可访问。
- 远程 tmux：通过 SSH 执行 tmux 命令读取远程 pane 输出。

### 8.2 状态识别

本地/远程终端输出中包含审批、确认、yes/no、permission、approval、continue 等关键词时，会尝试标记为：

```text
waiting_attention
```

已完成、空闲或无继续执行迹象时会尽量保持：

```text
running / done
```

具体判断逻辑在 `src-tauri/src/main.rs` 中的 terminal output 分析相关函数内。

### 8.3 Codex Desktop 支持

当前探索结论：

- Codex Desktop 是 Electron app。
- 主进程路径：

```text
/Applications/Codex.app/Contents/MacOS/Codex
```

- 相关 app-server：

```text
/Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled
```

- macOS Accessibility 能看到 Codex 窗口，但 UI 树很浅，基本是 `AXWindow -> AXGroup -> AXWebArea`，不可靠。
- 当前可靠方案：用 `ps` 检测 Codex Desktop 主进程，用 `~/.codex/logs_2.sqlite` 读取 app-server event。

当前监测事件：

```text
item/commandExecution/requestApproval
item/fileChange/requestApproval
item/permissions/requestApproval
item/tool/requestUserInput
mcpServer/elicitation/request
```

命中这些事件时标记为：

```text
waiting_attention
```

完成/解决类事件：

```text
turn/completed
serverRequest/resolved
thread/closed
```

命中后认为当前无需继续标红提醒。

当前前端展示：

- 名称：`Codex Desktop · Local Mac`
- 类型：`Codex Desktop`
- 按钮：`定位 Codex`
- target type：`desktop-app`

### 8.4 打开/定位终端

当前 target 类型：

- `local-shell`：新开本地 shell/tmux。
- `ssh-tmux`：通过 ssh 打开/附加远程 tmux。
- `local-process`：尽量定位已有终端进程；Terminal.app 目前可正确定位；Ghostty 之前测试无法稳定根据 pid 定位已有窗口，因此可能 fallback 到状态窗口。
- `desktop-app`：用于 Codex Desktop，目前通过 AppleScript/open 激活 Codex app。

## 9. 已知限制与风险

### 9.1 Codex Desktop 日志格式风险

Codex Desktop 监测依赖 `~/.codex/logs_2.sqlite` 中的内部事件名。未来 Codex Desktop 更新后，如果事件名或库位置变化，需要更新匹配规则。

### 9.2 Accessibility 不作为主方案

Codex Desktop 的 GUI 文本目前不能稳定从 Accessibility tree 读取。它适合作为“激活窗口/定位应用”的辅助手段，不适合作为审批状态主数据源。

### 9.3 远程状态依赖后台 SSH 认证

远程 tmux / 远程进程状态读取需要后台 SSH 可访问。优先使用公钥或 ControlMaster；如果用户确实使用 SSH 密码，Agent Pilot 会在对应 Agent 名称旁显示“需要 SSH 密码”，点击后可直接录入并保存到本机 `~/.agent-pilot/config.json`。通过卡片录入的密码默认只作为活跃 SSH 会话凭据保存，`scanEnabled` 会保持关闭，避免自动扫描整台服务器并生成大量 `remote_process` / `remote_tmux` 卡片。后端会优先使用 `sshpass`，本机没有 `sshpass` 时回退到临时 `SSH_ASKPASS` 脚本，命令结束后删除临时密码文件。

VS Code Remote-SSH 适配同样遵守这一限制：Agent Pilot 可以从 VS Code 本地进程中自动发现 Remote-SSH 目标 alias，并优先复用 VS Code 连接通道读取集成终端子进程；如果通道信息不可用且后台 SSH 需要密码，会在该 VS Code Remote-SSH Agent 上提示录入密码。点击 VS Code Remote-SSH 来源的 Agent 时，会优先定位 Visual Studio Code，而不是新开 tmux。

### 9.4 Ghostty 定位限制

Terminal.app 可以根据进程/TTY 更稳定地定位已有窗口；Ghostty 现在会读取 AppleScript `terminals` 暴露的 `id/name/working directory` 来辅助定位。对于 SSH + tmux 会话优先用远端 `tmux capture-pane` 监看；非 tmux SSH 会尝试远端进程扫描，若需要密码认证，会在对应 Agent 上引导用户录入。

### 9.5 dmg 分发限制

当前 dmg 未签名/公证。给别人使用时可能需要移除 quarantine，或者后续接入 Apple Developer ID 签名和 notarization。

### 9.6 前端 fallback mock

如果直接打开 `web/index.html` 或只跑静态 server 且后端 API 不在线，前端会显示 mock 数据。要看真实数据必须跑 Tauri dev 或确保 `127.0.0.1:8787` collector 在线。

## 10. 常用开发命令

### 10.1 启动桌面端

```bash
CARGO_TARGET_DIR=/private/tmp/agent-pilot-run-target cargo tauri dev
```

### 10.2 Rust 检查

```bash
CARGO_TARGET_DIR=/private/tmp/agent-pilot-check cargo check --manifest-path src-tauri/Cargo.toml
```

### 10.3 Rust 格式化检查

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

### 10.4 前端语法检查

```bash
node --check web/app.js
```

### 10.5 手动触发扫描

```bash
curl -fsS -X POST http://127.0.0.1:8787/api/discovery/scan \
  -H 'content-type: application/json' \
  -d '{"scope":"all"}'
```

### 10.6 查看当前状态

```bash
curl -fsS http://127.0.0.1:8787/api/state
```

### 10.7 查看 Codex Desktop 最近事件

```bash
sqlite3 ~/.codex/logs_2.sqlite \
  "select datetime(ts,'unixepoch','localtime'), substr(feedback_log_body,1,180)
   from logs
   where target='codex_app_server::outgoing_message'
     and feedback_log_body like 'app-server event:%'
   order by ts desc, ts_nanos desc
   limit 20;"
```

## 11. 新对话继续开发时的建议入口

建议在新对话中先让 Codex 读取：

```text
PROJECT_HANDOFF.md
README.md
src-tauri/src/main.rs
web/app.js
web/styles.css
src-tauri/tauri.conf.json
```

如果是 UI 任务，优先看：

```text
web/index.html
web/app.js
web/styles.css
web/assets/marvis-reference.png
```

如果是扫描/状态任务，优先看：

```text
src-tauri/src/main.rs
examples/config.example.json
scripts/claude-hook-example.sh
scripts/codex-notify-example.sh
```

## 12. 下一步可开发方向

### 12.1 Codex Desktop 深化

- 区分多个 Codex Desktop window/thread，而不是只显示一个全局 Codex Desktop。
- 从 `state_5.sqlite` / `logs_2.sqlite` 中关联 thread_id、窗口标题、当前项目路径。
- 增加更准确的审批“已解决”判断，避免历史 approval 事件残留造成误报。
- 添加状态解释：显示最近事件、最近 thread、是否等待 user input。

### 12.2 远程 Agent 深化

- 不仅识别 ssh 命令，还尝试解析远程 shell 当前前台程序。
- 对非 tmux 的 SSH 远程 Agent 支持更稳的状态采集。
- 增加远程主机健康检查 UI。

### 12.3 UI 主题扩展

- 保留当前 macOS 紧凑风格。
- 后续可新增第二套更动态/卡片化 UI，作为主题模式切换。
- 可以加入更细腻的动画，但建议先保持状态监测稳定。

### 12.4 分发完善

- 配置 Apple Developer ID。
- 自动签名、公证、生成可直接分发的 dmg。
- 增加首次启动权限引导。

## 13. 新对话可直接使用的开场提示

可以复制下面这段给新对话：

```text
请先阅读 /Users/van/MyFile/desktop ai/PROJECT_HANDOFF.md，继续 Agent Pilot 项目的开发。
项目是 Tauri v2 + Rust 后端 + 原生 HTML/CSS/JS 前端。
默认用中文回复，修改代码后请运行相关检查。
当前重点是继续完善桌面端 Agent 监测、UI 体验和 Codex Desktop 支持。
```
