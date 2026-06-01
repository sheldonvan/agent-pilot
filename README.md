# Agent Pilot

语言：简体中文 | [English](README.en.md)

多AI Agent 管理悬浮窗第一版，目前仅支持MacOS，支持本地终端、ghostty终端、ssh连接的服务器终端、tmux窗口等连接显示。若打开文件显示损坏，请打开终端输入xattr -dr com.apple.quarantine "/Applications/Agent Pilot.app"

Agent太多且不清楚多agent状态？本工具可查看各agent运行情况，多颜色状态区分agent运行情况，直观不麻烦。dmg文件版本进入release可下载，喜欢的话点个星支持一下！

<img width="430" height="650" alt="image" src="https://github.com/user-attachments/assets/9836db4d-d456-417d-b272-29ff3ae00aa7" />

Agent Pilot 是一个 macOS-first 的 AI Agent 管理悬浮窗，用来集中观察、定位和管理本机或远程终端里的 Claude Code、Codex CLI 等 Agent 会话。

> 当前版本：v0.2.0
>
> 下载：[Agent Pilot v0.2.0](https://github.com/sheldonvan/agent-pilot/releases/tag/v0.2.0)
>
> 许可证：代码采用 Apache-2.0；产品名称、图标、截图、发布视觉资产等品牌资产保留权利。

## 当前实现

- 列表式悬浮窗 UI：标题、统计、候选提示、Agent 卡片、底部状态栏。
- Discovery：启动扫描、自动扫描、本地进程扫描、本地 tmux 扫描、已配置远程 SSH/tmux 扫描。
- VS Code Remote-SSH：自动发现 VS Code 的 Remote-SSH `localServer.js` 会话，解析 SSH alias，并在该 alias 支持非交互认证时扫描远端 `claude/codex` 进程与 tmux pane。
- Ghostty / SSH：支持识别 Ghostty 中的本地 Agent、SSH Agent、SSH + tmux Agent，并尽量精确定位回对应终端。
- Agent 列表：检测到的新终端会自动加入，关闭/消失的已发现终端会自动移除。
- 候选 Agent：仅保留信息不完整、无法自动纳管的候选项，可确认添加或忽略。
- 手动添加 Agent：本地/远程、Ghostty/Terminal/iTerm2、tmux session、SSH 字段。
- 一键打开终端：本地 zsh + tmux，远程 SSH + tmux。
- 状态判断：优先使用审批提示、Claude 运行时状态、Codex Desktop 日志、tmux pane 信息和终端输出变化，避免仅凭输入框可输入就误判为“等待输入”。
- Collector API：`/api/events`、`/api/state`、`/api/discovery/*`、`/api/open-terminal`。
- 配置文件：`~/.agent-pilot/config.json`。

## 下载

当前 release 提供 macOS Apple Silicon dmg：

```text
Agent.Pilot_0.2.0_aarch64.dmg
```

下载地址：

```text
https://github.com/sheldonvan/agent-pilot/releases/download/v0.2.0/Agent.Pilot_0.2.0_aarch64.dmg
```

SHA-256：

```text
86771867833e47e40dd71dbf47c0b9376648d430b4eada529b90a4752ca1e21f
```

## 本地预览

```bash
python3 -m http.server 5174 -d web
```

然后打开 `http://127.0.0.1:5174`。

## Tauri 运行

当前前端不需要 npm/pnpm/yarn。桌面壳需要 Tauri CLI 和较新的 Rust 工具链。

推荐使用当前 stable Rust，然后安装 Tauri CLI：

```bash
cargo install tauri-cli --version "^2"
CARGO_TARGET_DIR=/private/tmp/agent-pilot-run-target cargo tauri dev
```

启动后 Collector 会监听：

```text
http://127.0.0.1:8787
```

主要 API：

```text
GET  /api/health
GET  /api/state
POST /api/events
POST /api/open-terminal
POST /api/discovery/scan
GET  /api/discovery/candidates
POST /api/discovery/confirm
POST /api/discovery/ignore
POST /api/agents/manual
```

## 远程扫描配置

参考 [examples/config.example.json](examples/config.example.json)，可将固定服务器写入 `~/.agent-pilot/config.json`。

VS Code Remote-SSH 和本地终端里的 `ssh` 会话不需要手动写入 `remoteHosts` 才能被发现；Agent Pilot 会从进程参数中读取 SSH 目标。若某个会话需要密码才能后台扫描，前端会在对应 Agent 名称旁显示“需要 SSH 密码”，点击后可直接录入，密码会自动保存到本机配置并用于后续后台 SSH 状态读取。通过卡片录入的密码默认只作为该会话的凭据，不会自动开启整台服务器的全局扫描，避免把同一台服务器上的所有进程拆成一堆卡片。公钥免密和 ControlMaster 仍然是更推荐的方式。

Ghostty 会话会读取其 AppleScript 暴露的 terminal `id`、`name` 和 `working directory` 来辅助定位。对于 `ssh` 后直接运行 Agent、`tmux`、或其他终端复用器的场景，Agent Pilot 会优先通过 SSH 远端进程扫描和可用的运行时状态文件判断状态；能解析出 tmux session 时会继续使用 `tmux capture-pane`。

## Hook / wrapper

Claude Code / Codex notify 可以参考：

```bash
scripts/claude-hook-example.sh
scripts/codex-notify-example.sh
```

也可以通过 wrapper 启动任意 Agent：

```bash
PILOT_AGENT_KIND=codex PILOT_AGENT_NAME="Local Codex" scripts/agent-pilot local-codex codex
```

## 许可证与版权

代码默认采用 [Apache License 2.0](LICENSE)。版权与署名信息见 [NOTICE](NOTICE)。

Agent Pilot 名称、品牌、图标、截图、release artwork 等视觉资产不随代码许可证授权复用，除非是合理署名或描述软件来源所必需。
