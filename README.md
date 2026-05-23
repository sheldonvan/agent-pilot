# Agent Pilot

macOS-first AI Agent 管理悬浮窗第一版。

## 当前实现

- 列表式悬浮窗 UI：标题、统计、候选提示、Agent 卡片、底部状态栏。
- Discovery：启动扫描、每分钟自动扫描、本地进程扫描、本地 tmux 扫描、已配置远程 SSH/tmux 扫描。
- Agent 列表：检测到的新终端会自动加入，关闭/消失的已发现终端会自动移除。
- 候选 Agent：仅保留信息不完整、无法自动纳管的候选项，可确认添加或忽略。
- 手动添加 Agent：本地/远程、Ghostty/Terminal/iTerm2、tmux session、SSH 字段。
- 一键打开终端：本地 zsh + tmux，远程 SSH + tmux。
- Collector API：`/api/events`、`/api/state`、`/api/discovery/*`、`/api/open-terminal`。
- 配置文件：`~/.agent-pilot/config.json`。

## 本地预览

```bash
python3 -m http.server 5174 -d web
```

然后打开 `http://127.0.0.1:5174`。

## Tauri 运行

当前前端不需要 npm/pnpm/yarn。桌面壳需要 Tauri CLI 和较新的 Rust 工具链。

本机验证时发现当前 `rustc 1.75.0` 无法编译 Tauri v2；请先升级到 `rustc 1.78+`，推荐直接安装当前 stable Rust，然后安装 Tauri CLI：

```bash
cargo install tauri-cli --version "^2"
cargo tauri dev
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

参考 [examples/config.example.json](examples/config.example.json)，将 `remoteHosts` 写入 `~/.agent-pilot/config.json`。

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
