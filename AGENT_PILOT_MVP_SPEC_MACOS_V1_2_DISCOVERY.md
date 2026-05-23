# Agent Pilot MVP 规格说明（macOS First + Agent 自动发现）

> 版本：v1.2  
> 状态：可直接交给 Codex 实现  
> 平台范围：**仅 macOS 第一版**  
> UI 方案：**采用已确认的列表式 Tauri 桌面悬浮窗方案**  
> 新增能力：**启动自动检测当前运行 Agent、自动检测新打开 Agent、手动刷新检测、手动添加 Agent**

---

## 0. 本版新增结论

本版在原有 “macOS Tauri 悬浮窗 + Agent 状态看板 + 一键打开对应终端” 的基础上，新增 **Agent Discovery / 自动发现机制**。

目标是做到：

1. App 下载并启动后，可以自动扫描当前用户正在运行的 Agent；
2. 能识别本地 macOS 上运行的 Claude Code / Codex CLI；
3. 能在配置了远程服务器后，扫描远程 Linux 服务器中的 Claude Code / Codex CLI / tmux 会话；
4. 当用户新开一个 Agent 时，App 能通过定时扫描、hook 上报或 wrapper 启动方式自动加入管理；
5. 用户可以点击“刷新检测”重新扫描；
6. 用户可以手动添加 Agent，解决自动识别不准或无法识别的场景。

但要明确一点：

**本地 Agent 可以较容易自动发现；远程服务器上的 Agent 不能在完全零配置情况下可靠发现。**

原因是远程 Agent 运行在 Linux 服务器上，macOS App 默认不知道你有哪些服务器、SSH 用户名、tmux session 名、项目路径，也无法直接访问远程进程列表。因此远程自动发现需要至少满足以下条件之一：

- 用户在 App 里配置过远程服务器；
- 用户当前 SSH 命令可被本地检测并解析出 host；
- 远程 Agent 已安装 Agent Pilot hook / notify 脚本；
- 用户通过 Agent Pilot wrapper 启动 Agent；
- 用户手动添加一次，之后由系统记住配置。

因此本版采用 **“自动发现 + 半自动确认 + 手动兜底”** 的设计。

---

## 1. 项目目标

Agent Pilot 是一个面向个人多 Agent 开发工作流的 **macOS 桌面悬浮窗看板**。

核心目标：

1. 统一查看多个 AI Agent 的当前状态；
2. 自动发现当前运行中的 Agent；
3. 自动纳管新启动的 Agent；
4. 及时发现哪些 Agent 正在运行、哪些需要关注、哪些已经完成；
5. 点击按钮一键打开对应终端；
6. 不做自动审批，不代替用户确认危险操作。

典型使用场景：

- 本地 macOS 使用 Ghostty；
- 通过 SSH 连接远程 Linux 服务器；
- 远程服务器里运行：
  - Codex CLI
  - Claude Code
- 本地 macOS 运行：
  - Claude Code
  - 其他杂活 Agent
- 用户开了多个终端后，希望有一个桌面悬浮窗统一显示每个 Agent 的运行状态。

---

## 2. MVP 范围

## 2.1 要做的功能

### A. macOS Tauri 桌面悬浮窗

- 使用 Tauri 构建 macOS 桌面应用；
- 以右侧悬浮窗形式显示；
- 窗口置顶；
- UI 采用已确认的列表式方案：
  - 顶部标题；
  - 顶部统计卡片；
  - 下方 Agent 列表卡片；
  - 底部更新时间和版本号。

### B. Agent 状态展示

每个 Agent 至少展示：

- Agent 名称；
- Agent 类型；
- 所在机器；
- 项目路径；
- 当前任务；
- 最近输出；
- 运行时长；
- 当前状态；
- 打开对应终端按钮。

### C. Agent 自动发现

新增 Discovery Manager，负责检测：

1. 本地正在运行的 Claude Code；
2. 本地正在运行的 Codex CLI；
3. 本地 tmux session 中的 Agent；
4. 已配置远程服务器上的 Claude Code / Codex CLI；
5. 已配置远程服务器上的 tmux session；
6. 通过 hook / notify 主动上报的新 Agent；
7. 通过 Agent Pilot wrapper 启动的新 Agent。

### D. 手动刷新检测

UI 提供“刷新检测”入口。

点击后触发：

- 本地进程扫描；
- 本地 tmux 扫描；
- 已配置远程服务器扫描；
- 合并检测结果；
- 新发现 Agent 进入 “Detected / 待确认” 或直接自动添加。

### E. 手动添加 Agent

UI 提供“手动添加 Agent”入口。

用户可填写：

- Agent 名称；
- Agent 类型；
- 本地 / 远程；
- 项目路径；
- 终端应用；
- tmux session；
- SSH host；
- SSH user；
- 打开终端命令模板。

### F. 一键打开对应终端

- 不做一键审批；
- 只做一键打开对应终端；
- 默认适配 Ghostty；
- 远程使用 SSH + tmux；
- 本地使用 zsh + tmux。

---

## 2.2 第一版不做

以下内容不属于本次 MVP：

1. 不做网页版 ChatGPT 集成；
2. 不做自动点击 yes；
3. 不做自动权限批准；
4. 不做跨平台 Windows / Linux 桌面版；
5. 不做复杂权限策略中心；
6. 不做完整终端内容回放；
7. 不做 OCR 识别终端屏幕；
8. 不做完全无配置远程扫描；
9. 不做对所有未知 CLI 工具的泛化识别；
10. 不做多用户登录和云同步。

---

## 3. 自动发现能力可行性分析

## 3.1 本地 Agent 自动发现

### 可行性：高

macOS 本地可以通过以下方式检测：

1. 进程扫描；
2. tmux session 扫描；
3. 启动命令 wrapper；
4. Claude Code / Codex hook 上报；
5. 心跳上报。

### 可检测信息

通常可以检测到：

- PID；
- 命令名称；
- 命令行参数；
- 启动时间；
- 当前用户；
- 进程状态；
- tmux session；
- tmux pane 当前目录；
- tmux pane 当前命令。

### 不一定可靠检测到的信息

- Agent 当前任务语义；
- 最近输出摘要；
- 是否正在等待审批；
- Agent 所属项目名称；
- Ghostty 具体窗口标签。

这些信息最好通过 hook / notify 上报补充。

---

## 3.2 远程 Agent 自动发现

### 可行性：中

远程发现可行，但需要前置条件。

### 可行方案

#### 方案 A：用户配置远程服务器后扫描

用户在 `~/.agent-pilot/config.json` 中配置：

```json
{
  "remoteHosts": [
    {
      "id": "server-main",
      "label": "Linux Server",
      "sshHost": "your.server.com",
      "sshUser": "root"
    }
  ]
}
```

App 点击刷新后执行：

```bash
ssh root@your.server.com "pgrep -af '(codex|claude)' || true"
ssh root@your.server.com "tmux list-panes -a -F '#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}' || true"
```

#### 方案 B：通过 SSH 反向端口转发 + hook 上报

用户 SSH 时带上：

```bash
ssh -R 8787:127.0.0.1:8787 root@your.server.com
```

远程 Agent 通过 hook / notify 直接上报：

```bash
curl -X POST http://127.0.0.1:8787/api/events \
  -H "Content-Type: application/json" \
  -d '{...}'
```

#### 方案 C：通过 Agent Pilot wrapper 启动远程 Agent

例如：

```bash
agent-pilot run --id remote-codex-project-a -- codex
agent-pilot run --id remote-claude-project-b -- claude
```

wrapper 启动时自动注册 Agent，并定时发送 heartbeat。

### 远程发现的限制

无法做到完全零配置自动扫描远程服务器，因为 App 无法天然知道：

- 有哪些服务器；
- SSH 用户名；
- SSH 端口；
- 是否需要跳板机；
- 是否有权限执行 pgrep / tmux；
- Agent 是否运行在 tmux 之外；
- 当前 SSH 是否存在反向端口转发。

因此远程发现采用：

```text
已配置服务器扫描 + hook 主动上报 + 手动添加兜底
```

---

## 3.3 新打开 Agent 自动检测

### 可行性：高，但推荐组合策略

新 Agent 的自动检测可通过三条路径实现：

### 路径 1：定时扫描

App 每隔一段时间扫描本地和已配置远程环境。

建议频率：

- 本地扫描：每 15 秒；
- 远程扫描：每 30 ~ 60 秒；
- 用户点击刷新：立即扫描。

### 路径 2：hook / notify 主动上报

Claude Code / Codex 启动、任务更新、完成、等待关注时主动 POST 到 Collector。

这是最准确的方式。

### 路径 3：Agent Pilot wrapper 启动

用户以后可以用：

```bash
agent-pilot claude --agent-id local-claude-misc
agent-pilot codex --agent-id remote-codex-project-a
```

或者：

```bash
agent-pilot run claude
agent-pilot run codex
```

这样 App 可以 100% 知道新 Agent 被启动了。

### 推荐优先级

```text
hook / notify 上报 > wrapper 启动 > tmux 扫描 > 进程扫描
```

---

## 4. Discovery Manager 设计

## 4.1 模块职责

Discovery Manager 负责：

1. 启动时自动扫描；
2. 定时扫描；
3. 手动刷新扫描；
4. 合并检测结果；
5. 判断是否是新 Agent；
6. 判断是否与已有 Agent 重复；
7. 自动添加或进入待确认列表；
8. 更新 Agent 在线 / 离线状态。

---

## 4.2 Discovery 数据流

```text
App 启动
  ↓
加载 ~/.agent-pilot/config.json
  ↓
Discovery Manager 开始扫描
  ├─ LocalProcessScanner
  ├─ LocalTmuxScanner
  ├─ RemoteHostScanner
  ├─ HookEventRegistry
  └─ ManualAgentRegistry
  ↓
生成 DiscoveredAgent[]
  ↓
Dedup / Merge
  ↓
已知 Agent → 更新状态
未知 Agent → 自动添加或进入待确认列表
  ↓
UI 刷新
```

---

## 4.3 Discovery 来源

### A. local_process

通过 macOS 本地进程扫描发现。

示例命令：

```bash
pgrep -af '(^|/)(claude|codex)( |$)'
```

或者：

```bash
ps aux | grep -E 'claude|codex'
```

### B. local_tmux

通过本地 tmux 扫描发现。

示例：

```bash
tmux list-panes -a -F '#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}'
```

### C. remote_process

通过 SSH 登录远程服务器扫描进程。

示例：

```bash
ssh root@your.server.com "pgrep -af '(codex|claude)' || true"
```

### D. remote_tmux

通过 SSH 登录远程服务器扫描 tmux。

示例：

```bash
ssh root@your.server.com "tmux list-panes -a -F '#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}' || true"
```

### E. hook_report

由 Agent hook / notify 主动上报。

### F. wrapper_report

由 `agent-pilot run` 主动注册。

### G. manual

用户手动添加。

---

## 4.4 DiscoveredAgent 数据结构

```ts
export interface DiscoveredAgent {
  fingerprint: string;
  agentId?: string;
  name?: string;
  kind: 'codex' | 'claude_code' | 'unknown';
  location: 'local' | 'remote';
  machineLabel: string;
  cwd?: string;
  command?: string;
  pid?: number;
  tmuxSession?: string;
  tmuxPane?: string;
  sshHost?: string;
  sshUser?: string;
  discoverySource:
    | 'local_process'
    | 'local_tmux'
    | 'remote_process'
    | 'remote_tmux'
    | 'hook_report'
    | 'wrapper_report'
    | 'manual';
  confidence: number;
  detectedAt: string;
}
```

---

## 4.5 指纹 fingerprint 规则

fingerprint 用于判断两个检测结果是否是同一个 Agent。

### 本地进程 Agent

```text
local:{kind}:{pid}:{command}
```

### 本地 tmux Agent

```text
local-tmux:{kind}:{sessionName}:{paneIndex}:{cwd}
```

### 远程 tmux Agent

```text
remote-tmux:{sshUser}@{sshHost}:{kind}:{sessionName}:{paneIndex}:{cwd}
```

### hook 主动上报 Agent

```text
hook:{agentId}
```

### 手动添加 Agent

```text
manual:{agentId}
```

---

## 4.6 去重与合并规则

同一个 Agent 可能同时被多个来源检测到，例如：

- 进程扫描发现；
- tmux 扫描发现；
- hook 上报发现。

合并优先级：

```text
hook_report > wrapper_report > manual > local_tmux/remote_tmux > local_process/remote_process
```

### 合并原则

1. 如果 `agentId` 相同，直接合并；
2. 如果 fingerprint 相同，直接合并；
3. 如果 `tmuxSession + cwd + kind` 相同，认为高度可能是同一个；
4. 如果只有 command 相同但 cwd 不同，不合并；
5. 如果无法确认，进入“待确认”列表。

---

## 5. 自动添加策略

## 5.1 两种模式

配置项：

```json
{
  "discovery": {
    "autoAddMode": "confirm"
  }
}
```

支持：

### confirm

发现新 Agent 后进入待确认区，用户点击“添加到管理”。

适合 MVP 默认模式。

### trusted

对高置信度 Agent 自动添加。

例如：

- 通过 hook 上报；
- 通过 wrapper 启动；
- 命中配置中的 tmux session；
- 命中用户之前添加过的 Agent。

---

## 5.2 推荐 MVP 默认行为

第一版默认使用：

```json
{
  "autoAddMode": "confirm"
}
```

即：

- 已配置 Agent：自动更新；
- hook/wrapper 上报：自动添加；
- 扫描发现但不确定：进入“检测到的新 Agent”；
- 用户点击确认后加入管理。

这样能避免误把普通 shell、测试进程、历史 tmux session 识别成 Agent。

---

## 6. UI 更新设计

## 6.1 新增按钮

在原 UI 右上角或底部增加两个入口：

1. `刷新检测`
2. `手动添加`

推荐放在右上角更多菜单中：

```text
[设置] [...]
       ├─ 刷新检测
       ├─ 手动添加 Agent
       ├─ 打开配置文件
       └─ 退出
```

---

## 6.2 待确认 Agent 区域

当发现未纳管 Agent 时，在统计区下方或列表顶部显示一条轻量提示：

```text
检测到 2 个新 Agent    [查看] [全部忽略]
```

点击“查看”后展示：

```text
┌────────────────────────────────────┐
│ 检测到的新 Agent                   │
├────────────────────────────────────┤
│ Claude Code                        │
│ Local Mac · ~/Desktop/misc         │
│ 来源：local_tmux · 置信度 0.88      │
│ [添加到管理] [忽略]                 │
├────────────────────────────────────┤
│ Codex CLI                          │
│ Linux Server · /workspace/project-a│
│ 来源：remote_tmux · 置信度 0.81     │
│ [添加到管理] [忽略]                 │
└────────────────────────────────────┘
```

---

## 6.3 Agent 卡片新增来源标识

每张 Agent 卡片可增加一个很小的来源标签，例如：

- hook
- tmux
- manual
- wrapper

示例：

```text
Remote Codex · 项目A        等待关注
来源：remote_tmux + hook
```

MVP 可选，不强制。

---

## 6.4 手动添加弹窗字段

### 必填字段

- Agent 名称；
- Agent 类型；
- 本地 / 远程；
- 终端应用；
- tmux session 名称。

### 本地 Agent 字段

- 项目路径；
- 本地启动命令；
- tmux session。

### 远程 Agent 字段

- SSH Host；
- SSH User；
- SSH Port；
- 项目路径；
- tmux session；
- 远程 attach 命令。

---

## 7. 新增 API 设计

## 7.1 POST /api/discovery/scan

手动触发扫描。

### Request

```json
{
  "scope": "all"
}
```

scope 可选：

- `all`
- `local`
- `remote`
- `tmux`
- `process`

### Response

```json
{
  "ok": true,
  "detectedCount": 3,
  "newCount": 1
}
```

---

## 7.2 GET /api/discovery/candidates

获取待确认 Agent。

### Response

```json
{
  "candidates": [
    {
      "fingerprint": "local-tmux:claude_code:local_claude_misc:0:~/Desktop/misc",
      "kind": "claude_code",
      "location": "local",
      "machineLabel": "Local Mac",
      "cwd": "~/Desktop/misc",
      "tmuxSession": "local_claude_misc",
      "discoverySource": "local_tmux",
      "confidence": 0.88,
      "detectedAt": "2026-05-22T14:36:12+08:00"
    }
  ]
}
```

---

## 7.3 POST /api/discovery/confirm

将候选 Agent 添加到管理。

### Request

```json
{
  "fingerprint": "local-tmux:claude_code:local_claude_misc:0:~/Desktop/misc",
  "name": "Local Claude Code · 杂活"
}
```

### Response

```json
{
  "ok": true,
  "agentId": "local-claude-code-misc"
}
```

---

## 7.4 POST /api/discovery/ignore

忽略某个候选 Agent。

### Request

```json
{
  "fingerprint": "local-tmux:claude_code:test:0:/tmp"
}
```

### Response

```json
{
  "ok": true
}
```

---

## 7.5 POST /api/agents/manual

手动添加 Agent。

### Request 示例：远程 Agent

```json
{
  "name": "Remote Codex · 项目A",
  "kind": "codex",
  "location": "remote",
  "machineLabel": "Linux Server",
  "cwd": "/workspace/project-a",
  "terminalTarget": {
    "type": "ssh-tmux",
    "terminalApp": "ghostty",
    "sshHost": "your.server.com",
    "sshUser": "root",
    "sshPort": 22,
    "sessionName": "codex_project_a",
    "remoteCommand": "tmux attach -t codex_project_a || tmux new -s codex_project_a"
  }
}
```

### Request 示例：本地 Agent

```json
{
  "name": "Local Claude Code · 杂活",
  "kind": "claude_code",
  "location": "local",
  "machineLabel": "Local Mac",
  "cwd": "~/Desktop/misc",
  "terminalTarget": {
    "type": "local-shell",
    "terminalApp": "ghostty",
    "sessionName": "local_claude_misc",
    "localCommand": "cd ~/Desktop/misc && tmux attach -t local_claude_misc || tmux new -s local_claude_misc"
  }
}
```

---

## 8. 配置文件更新

配置文件路径：

```text
~/.agent-pilot/config.json
```

### 新版示例

```json
{
  "app": {
    "listenHost": "127.0.0.1",
    "listenPort": 8787,
    "offlineTimeoutSec": 120,
    "defaultTerminal": "ghostty"
  },
  "discovery": {
    "enabled": true,
    "scanOnStartup": true,
    "localScanIntervalSec": 15,
    "remoteScanIntervalSec": 45,
    "autoAddMode": "confirm",
    "scanLocalProcesses": true,
    "scanLocalTmux": true,
    "scanRemoteHosts": true,
    "trustedCommands": ["claude", "codex"],
    "ignoredFingerprints": []
  },
  "remoteHosts": [
    {
      "id": "server-main",
      "label": "Linux Server",
      "sshHost": "your.server.com",
      "sshUser": "root",
      "sshPort": 22,
      "scanEnabled": true
    }
  ],
  "agents": []
}
```

---

## 9. Collector / 后端模块更新

推荐目录：

```text
src-tauri/
  src/
    main.rs
    app_state.rs
    config.rs
    models.rs
    collector/
      mod.rs
      routes.rs
      reducer.rs
      heartbeat.rs
    discovery/
      mod.rs
      manager.rs
      local_process.rs
      local_tmux.rs
      remote_process.rs
      remote_tmux.rs
      dedup.rs
      confidence.rs
      candidates.rs
    commands/
      mod.rs
      open_terminal.rs
    storage/
      mod.rs
      sqlite.rs
```

---

## 10. Discovery 实现细节

## 10.1 LocalProcessScanner

### 职责

扫描 macOS 当前用户进程中是否存在：

- claude
- codex
- claude-code
- codex-cli

### 命令示例

```bash
pgrep -af 'claude|codex'
```

### Rust 实现建议

通过 `std::process::Command` 调用系统命令。

MVP 不需要直接调用复杂 macOS 原生 API。

### 置信度规则

- 命令名精确匹配 `claude` 或 `codex`：0.70
- 命令行包含项目路径：+0.10
- 进程在当前用户下运行：+0.05
- 同时被 tmux 扫描发现：合并后提升到 0.85+

---

## 10.2 LocalTmuxScanner

### 职责

扫描本地 tmux session 中是否存在 Agent。

### 命令

```bash
tmux list-panes -a -F '#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}'
```

### 识别规则

如果 `pane_current_command` 包含：

- claude
- codex
- node / bun / python 包裹但 session 名包含 claude/codex

则识别为候选 Agent。

### 置信度规则

- pane_current_command 精确为 claude/codex：0.88
- session 名包含 claude/codex：0.80
- 当前路径不为空：+0.05

---

## 10.3 RemoteTmuxScanner

### 职责

扫描已配置服务器上的 tmux。

### 命令

```bash
ssh -p 22 root@your.server.com \
  "tmux list-panes -a -F '#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}' || true"
```

### 注意

- 必须设置超时；
- 不要阻塞 UI；
- SSH 扫描失败不要报错弹窗，只记录状态；
- 对每台远程服务器单独维护 scan status。

---

## 10.4 RemoteProcessScanner

### 职责

扫描远程服务器进程。

### 命令

```bash
ssh -p 22 root@your.server.com "pgrep -af '(claude|codex)' || true"
```

### 注意

进程扫描不如 tmux 扫描可靠，因为它不一定能拿到项目路径和 session 名。

MVP 中建议 remote_process 作为补充来源，不作为最主要来源。

---

## 10.5 HookEventRegistry

### 职责

当 `/api/events` 收到新 `agentId` 时：

- 如果 agentId 已存在，更新状态；
- 如果 agentId 不存在，但事件信息完整，自动添加；
- 如果事件信息不完整，加入候选列表。

### 自动添加条件

满足以下条件可自动添加：

- event 中包含 agentId；
- event 中包含 name；
- event 中包含 kind；
- event 中包含 cwd；
- event 中包含 terminalTarget 或能从配置推导出 terminalTarget。

---

## 10.6 ManualAgentRegistry

### 职责

保存用户手动添加的 Agent，并优先保持其配置。

手动添加的 Agent 不应被扫描结果随意覆盖关键配置。

---

## 11. 状态机更新

原状态：

```ts
export type AgentStatus =
  | 'running'
  | 'waiting_attention'
  | 'done'
  | 'error'
  | 'offline'
  | 'unknown';
```

建议新增：

```ts
export type AgentDiscoveryState =
  | 'managed'
  | 'candidate'
  | 'ignored';
```

### AgentItem 新增字段

```ts
export interface AgentItem {
  id: string;
  name: string;
  kind: 'codex' | 'claude_code' | 'other';
  location: 'local' | 'remote';
  machineLabel: string;
  projectLabel?: string;
  cwd?: string;
  currentTask?: string;
  lastOutput?: string;
  status: AgentStatus;
  discoveryState: AgentDiscoveryState;
  discoverySources: string[];
  confidence?: number;
  pid?: number;
  tmuxSession?: string;
  tmuxPane?: string;
  sshHost?: string;
  sshUser?: string;
  startedAt?: string;
  updatedAt: string;
  durationSec?: number;
  terminalTarget?: TerminalTarget;
}
```

---

## 12. UI 文案建议

### 顶部更多菜单

```text
刷新检测
手动添加 Agent
打开配置文件
打开日志目录
退出
```

### 新 Agent 提示

```text
检测到 2 个新 Agent
```

按钮：

```text
查看
忽略
```

### 候选 Agent 卡片

```text
检测到 Claude Code
Local Mac · ~/Desktop/misc
来源：tmux · 置信度 88%
[添加到管理] [忽略]
```

### 扫描中状态

```text
正在检测本地与远程 Agent...
```

### 远程扫描失败

```text
Linux Server 暂时无法扫描
```

不要强弹错误窗口，避免打扰用户。

---

## 13. 启动流程

App 启动后执行：

```text
1. 加载配置
2. 启动 Collector API
3. 启动 UI
4. 启动 Discovery Manager
5. 如果 scanOnStartup=true：
   5.1 扫描本地进程
   5.2 扫描本地 tmux
   5.3 扫描已配置远程服务器
6. 合并结果
7. 自动添加可信 Agent
8. 将不确定 Agent 放入候选列表
9. UI 展示状态
10. 后台定时扫描
```

---

## 14. 新打开 Agent 的识别流程

当用户新开一个终端并启动 Claude Code / Codex：

### 情况 A：通过 hook / notify

```text
Agent 启动
  ↓
hook 上报 session_started
  ↓
Collector 收到新 agentId
  ↓
自动添加或更新
  ↓
UI 出现新卡片
```

### 情况 B：通过 tmux

```text
用户在 tmux session 里启动 claude/codex
  ↓
Discovery 定时扫描 tmux
  ↓
发现新 pane command
  ↓
生成候选 Agent
  ↓
用户确认添加
```

### 情况 C：直接在 Ghostty 普通 shell 启动

```text
用户直接运行 claude/codex
  ↓
LocalProcessScanner 检测到进程
  ↓
由于缺少项目路径/session，进入候选列表
  ↓
用户补充名称和打开方式
```

---

## 15. 手动刷新流程

用户点击：

```text
更多菜单 → 刷新检测
```

系统执行：

```text
1. UI 显示 scanning 状态
2. 调用 POST /api/discovery/scan
3. 执行本地扫描
4. 执行远程扫描
5. 合并结果
6. 更新 managed agents
7. 更新 candidates
8. UI 刷新
```

---

## 16. 手动添加流程

用户点击：

```text
更多菜单 → 手动添加 Agent
```

弹窗字段：

### 基础信息
- Agent 名称；
- Agent 类型；
- 本地 / 远程；
- 项目路径；
- 当前任务描述，可选。

### 终端信息
- 终端应用：Ghostty / Terminal.app / iTerm2；
- tmux session；
- 本地命令或远程命令。

### 远程信息
- SSH Host；
- SSH User；
- SSH Port。

保存后：

```text
1. 写入 config.json
2. 写入 SQLite
3. 加入 managed agents
4. UI 立即显示
```

---

## 17. 新增开发里程碑

## Milestone 1：静态 UI

目标：完成确认版 UI。

### 内容
- Tauri + React + TypeScript；
- 悬浮窗；
- Header；
- Summary；
- AgentCard；
- Footer；
- Mock 数据。

---

## Milestone 2：Collector API

目标：通过 API 更新 Agent 状态。

### 内容
- POST /api/events；
- GET /api/state；
- GET /api/health；
- 前端轮询或 WebSocket；
- 状态统计。

---

## Milestone 3：一键打开对应终端

目标：按钮能打开 Ghostty 并进入对应 tmux session。

### 内容
- Tauri command；
- local-shell；
- ssh-tmux；
- 白名单命令模板。

---

## Milestone 4：Agent Discovery MVP

目标：启动自动发现和手动刷新检测。

### 内容
- LocalProcessScanner；
- LocalTmuxScanner；
- RemoteTmuxScanner；
- RemoteProcessScanner；
- Discovery candidate 列表；
- Refresh 按钮；
- Confirm / Ignore 逻辑。

---

## Milestone 5：手动添加 Agent

目标：用户可以手动纳管无法识别的 Agent。

### 内容
- 添加弹窗；
- 配置保存；
- 卡片立即显示；
- 打开终端测试。

---

## Milestone 6：Hook / Notify 接入

目标：让真实 Codex / Claude Code 更准确上报。

### 内容
- Claude Code hook 示例；
- Codex notify 示例；
- heartbeat 脚本；
- agentId 标准化。

---

## 18. 验收标准

MVP 完成应满足：

1. macOS 可以启动 Tauri 应用；
2. UI 采用确认的列表式悬浮窗方案；
3. 启动后自动扫描本地 Agent；
4. 支持扫描本地 tmux；
5. 支持扫描已配置远程服务器 tmux；
6. 支持手动刷新检测；
7. 支持显示待确认 Agent；
8. 支持将候选 Agent 添加到管理；
9. 支持忽略候选 Agent；
10. 支持手动添加 Agent；
11. 支持通过 `/api/events` 更新状态；
12. 支持一键打开对应终端；
13. 不做自动审批；
14. 不做网页版 ChatGPT；
15. 远程发现失败时不影响本地功能。

---

## 19. 给 Codex 的新版实现提示词

下面这段可以直接交给 Codex：

```text
你现在要帮我实现一个 macOS first 的 Tauri 桌面应用 MVP，项目名叫 Agent Pilot。

这个应用是一个 AI Agent 管理悬浮窗，用于管理我本地和远程服务器上正在运行的 Codex CLI、Claude Code 等 Agent。

重要要求：
1. 第一版只做 macOS。
2. UI 必须采用列表式悬浮窗方案：顶部标题栏、顶部统计卡片、Agent 卡片列表、底部状态栏。
3. 每个 Agent 卡片显示：名称、状态、当前任务、机器、路径、最近输出、运行时长、打开对应终端按钮。
4. 不做网页版 ChatGPT。
5. 不做一键审批。
6. 只做一键打开对应终端。
7. 默认终端是 Ghostty。
8. 远程会话通过 SSH + tmux 打开。
9. 本地会话通过 zsh + tmux 打开。

新增重点功能：
1. App 启动后自动检测当前用户正在运行的 Agent。
2. 支持检测本地 macOS 进程中的 claude/codex。
3. 支持检测本地 tmux session 中的 claude/codex。
4. 支持检测已配置远程服务器上的 tmux session 和 claude/codex 进程。
5. 支持后台定时检测新打开的 Agent。
6. 支持用户点击“刷新检测”手动扫描。
7. 支持用户点击“手动添加 Agent”添加无法自动识别的 Agent。
8. 自动发现的新 Agent 如果置信度不足，先进入候选列表，用户确认后加入管理。
9. 通过 hook/notify 或 wrapper 上报的新 Agent 可以自动加入管理。

技术栈：
- Tauri v2
- React + TypeScript
- Rust backend
- Axum 本地 Collector API
- SQLite 或本地 JSON 配置

需要实现的 API：
- POST /api/events
- GET /api/state
- GET /api/health
- POST /api/open-terminal
- POST /api/discovery/scan
- GET /api/discovery/candidates
- POST /api/discovery/confirm
- POST /api/discovery/ignore
- POST /api/agents/manual

请按以下顺序实现：
1. 创建项目结构；
2. 实现静态 UI；
3. 实现 Collector API；
4. 实现打开 Ghostty + tmux；
5. 实现 LocalProcessScanner；
6. 实现 LocalTmuxScanner；
7. 实现 RemoteTmuxScanner；
8. 实现候选 Agent 列表和确认添加；
9. 实现手动添加 Agent；
10. 补充 hook / notify 示例脚本。

请先输出项目目录结构、核心数据模型、Discovery 设计、API 设计，然后开始逐步生成代码。
```

---

## 20. 最终建议

为了让第一版既实用又稳定，推荐采用以下策略：

```text
默认自动扫描本地
远程扫描需要用户配置 host
hook / wrapper 上报自动加入
扫描发现但不确定的 Agent 进入待确认列表
用户可以手动刷新检测
用户可以手动添加 Agent
```

这样既能做到“下载启动后尽可能自动发现”，又不会因为识别错误导致管理界面混乱。

本版的核心产品逻辑可以总结为：

```text
看得见当前 Agent
找得到新开的 Agent
不确定就让用户确认
点一下回到对应终端
```
