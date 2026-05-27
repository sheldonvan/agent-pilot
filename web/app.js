const tauriInvoke = window.__TAURI__?.core?.invoke;
const apiBase = "http://127.0.0.1:8787/api";
const dragIgnoreSelector = "button,input,select,textarea,a,[role='button'],dialog";
const autoRefreshIntervalMs = 60_000;
const themeStorageKey = "agentPilot.themeMode";
const permissionGuideStorageKey = "agentPilot.permissionGuide.v1";

const els = {
  runningCount: document.querySelector("#runningCount"),
  attentionCount: document.querySelector("#attentionCount"),
  agentList: document.querySelector("#agentList"),
  lastUpdated: document.querySelector("#lastUpdated"),
  refreshBtn: document.querySelector("#refreshBtn"),
  settingsBtn: document.querySelector("#settingsBtn"),
  addBtn: document.querySelector("#addBtn"),
  settingsDialog: document.querySelector("#settingsDialog"),
  permissionDialog: document.querySelector("#permissionDialog"),
  permissionGuideBtn: document.querySelector("#permissionGuideBtn"),
  skipPermissionBtn: document.querySelector("#skipPermissionBtn"),
  finishPermissionBtn: document.querySelector("#finishPermissionBtn"),
  candidateDialog: document.querySelector("#candidateDialog"),
  candidateList: document.querySelector("#candidateList"),
  manualDialog: document.querySelector("#manualDialog"),
  manualForm: document.querySelector("#manualForm"),
  closeManualBtn: document.querySelector("#closeManualBtn"),
  sshPasswordDialog: document.querySelector("#sshPasswordDialog"),
  sshPasswordForm: document.querySelector("#sshPasswordForm"),
  closeSshPasswordBtn: document.querySelector("#closeSshPasswordBtn"),
  sshPasswordTarget: document.querySelector("#sshPasswordTarget"),
  locationSelect: document.querySelector("#locationSelect"),
};

let themeMode = localStorage.getItem(themeStorageKey) || "system";

let state = {
  version: "0.1.1",
  scanning: false,
  collectorOnline: false,
  collectorUrl: apiBase,
  lastUpdated: new Date().toISOString(),
  scanReports: [
    {
      source: "preview",
      label: "静态预览",
      ok: true,
      message: "未连接桌面后端时使用演示数据",
      checkedAt: new Date().toISOString(),
    },
  ],
  agents: [
    {
      id: "demo-local-codex",
      name: "Local Codex · Pilot",
      kind: "codex",
      location: "local",
      machineLabel: "Local Mac",
      cwd: "~/MyFile/desktop ai",
      currentTask: "按照规格生成第一版 App",
      lastOutput: "UI 草图、Discovery 候选区、手动添加流程已就绪。",
      status: "running",
      discoveryState: "managed",
      discoverySources: ["manual"],
      confidence: 0.92,
      startedAt: new Date(Date.now() - 42 * 60 * 1000).toISOString(),
      updatedAt: new Date().toISOString(),
      durationSec: 42 * 60,
      tmuxSession: "agent_pilot_desk",
      terminalTarget: {
        type: "local-shell",
        terminalApp: "ghostty",
        sessionName: "agent_pilot_desk",
        localCommand: "cd ~/MyFile/desktop\\ ai && tmux attach -t agent_pilot_desk || tmux new -s agent_pilot_desk",
      },
    },
    {
      id: "demo-remote-claude",
      name: "Remote Claude · API",
      kind: "claude_code",
      location: "remote",
      machineLabel: "Linux Server",
      cwd: "/workspace/project-a",
      currentTask: "等待用户确认远程配置",
      lastOutput: "远程扫描需要在配置文件里添加 sshHost 和 sshUser。",
      status: "waiting_attention",
      discoveryState: "managed",
      discoverySources: ["manual"],
      confidence: 0.8,
      sshHost: "your.server.com",
      sshUser: "root",
      sshPort: 22,
      sshPasswordRequired: true,
      startedAt: new Date(Date.now() - 18 * 60 * 1000).toISOString(),
      updatedAt: new Date().toISOString(),
      durationSec: 18 * 60,
      tmuxSession: "claude_api",
      terminalTarget: {
        type: "ssh-tmux",
        terminalApp: "ghostty",
        sshHost: "your.server.com",
        sshUser: "root",
        sshPort: 22,
        sessionName: "claude_api",
        remoteCommand: "tmux attach -t claude_api || tmux new -s claude_api",
      },
    },
  ],
  candidates: [
    {
      fingerprint: "local-tmux:codex:agent_pilot_desk:0:~/MyFile/desktop ai",
      kind: "codex",
      location: "local",
      machineLabel: "Local Mac",
      cwd: "~/MyFile/desktop ai",
      command: "codex",
      tmuxSession: "agent_pilot_desk",
      tmuxPane: "0",
      discoverySource: "local_tmux",
      confidence: 0.88,
      detectedAt: new Date().toISOString(),
    },
  ],
};
let autoRefreshTimer;
let refreshInFlight = false;
let lastScanResult;
let editingAgentId = null;
let editingAgentName = "";

function getCurrentTauriWindow() {
  return window.__TAURI__?.window?.getCurrentWindow?.() || window.__TAURI__?.window?.appWindow;
}

function setupWindowDragging() {
  const dragArea = document.querySelector(".window-header");
  const appWindow = getCurrentTauriWindow();
  if (!dragArea || !appWindow?.startDragging) return;

  const startDrag = (event) => {
    if (event.button !== 0) return;
    if (event.target.closest(dragIgnoreSelector)) return;

    event.preventDefault();
    window.getSelection?.()?.removeAllRanges();
    appWindow.startDragging().catch(() => {});
  };

  dragArea.addEventListener("mousedown", startDrag, { capture: true });

  dragArea.addEventListener("selectstart", (event) => {
    if (!event.target.closest(dragIgnoreSelector)) {
      event.preventDefault();
    }
  });
}

function applyTheme(mode = themeMode) {
  themeMode = ["system", "dark", "light"].includes(mode) ? mode : "system";
  localStorage.setItem(themeStorageKey, themeMode);
  document.documentElement.dataset.themeMode = themeMode;
  document.querySelectorAll("input[name='themeMode']").forEach((input) => {
    input.checked = input.value === themeMode;
  });
}

async function invoke(command, args = {}) {
  if (tauriInvoke) {
    try {
      return await tauriInvoke(command, args);
    } catch (error) {
      return httpInvoke(command, args).catch(() => mockInvoke(command, args));
    }
  }
  return httpInvoke(command, args).catch(() => mockInvoke(command, args));
}

async function httpInvoke(command, args = {}) {
  const routes = {
    get_state: ["GET", "/state"],
    discovery_scan: ["POST", "/discovery/scan", { scope: args.scope || "all" }],
    confirm_candidate: ["POST", "/discovery/confirm", args],
    ignore_candidate: ["POST", "/discovery/ignore", args],
    manual_agent: ["POST", "/agents/manual", { agent: args.agent }],
    rename_agent: ["POST", "/agents/rename", { id: args.id, name: args.name }],
    save_ssh_password: ["POST", "/ssh/password", args],
    post_event: ["POST", "/events", args.event || args],
    open_terminal: ["POST", "/open-terminal", { target: args.target }],
    open_permission_settings: ["POST", "/permissions/open", { pane: args.pane }],
  };
  const route = routes[command];
  if (!route) throw new Error(`No HTTP route for ${command}`);
  const [method, path, body] = route;
  const response = await fetch(`${apiBase}${path}`, {
    method,
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

async function mockInvoke(command, args) {
  await new Promise((resolve) => setTimeout(resolve, command === "discovery_scan" ? 420 : 80));
  if (command === "get_state") return state;
  if (command === "discovery_scan") {
    state = {
      ...state,
      scanning: false,
      lastUpdated: new Date().toISOString(),
      agents: state.agents.filter((agent) => agent.status !== "offline"),
      candidates: [],
    };
    return { ok: true, detectedCount: state.agents.length, newCount: 0, removedCount: 0 };
  }
  if (command === "confirm_candidate") {
    const candidate = state.candidates.find((item) => item.fingerprint === args.fingerprint);
    if (!candidate) return { ok: false };
    state.candidates = state.candidates.filter((item) => item.fingerprint !== args.fingerprint);
    state.agents = [agentFromCandidate(candidate, args.name), ...state.agents];
    return { ok: true };
  }
  if (command === "ignore_candidate") {
    state.candidates = state.candidates.filter((item) => item.fingerprint !== args.fingerprint);
    return { ok: true };
  }
  if (command === "manual_agent") {
    state.agents = [args.agent, ...state.agents];
    return { ok: true };
  }
  if (command === "rename_agent") {
    const name = args.name?.trim();
    if (!name) return { ok: false };
    state.agents = state.agents.map((agent) => (agent.id === args.id ? { ...agent, name } : agent));
    return { ok: true };
  }
  if (command === "save_ssh_password") {
    state.agents = state.agents.map((agent) =>
      agent.sshHost === args.sshHost && Number(agent.sshPort || 22) === Number(args.sshPort || 22)
        ? { ...agent, sshPasswordRequired: false, lastOutput: "SSH 密码已保存到本机配置。" }
        : agent,
    );
    return { ok: true };
  }
  if (command === "post_event") return { ok: true };
  if (command === "open_terminal" || command === "open_config_file" || command === "open_permission_settings") return { ok: true };
  return null;
}

function agentFromCandidate(candidate, name) {
  const kindLabel = candidate.kind === "claude_code" ? "Claude Code" : candidate.kind === "codex" ? "Codex CLI" : "Agent";
  const session = candidate.tmuxSession || `${candidate.kind}_session`;
  return {
    id: candidate.fingerprint.replace(/[^a-zA-Z0-9]+/g, "-"),
    name: name || `${kindLabel} · ${candidate.machineLabel}`,
    kind: candidate.kind,
    location: candidate.location,
    machineLabel: candidate.machineLabel,
    cwd: candidate.cwd,
    currentTask: "刚加入管理，等待下一次状态上报。",
    lastOutput: candidate.command || "由 Discovery 扫描发现。",
    status: "running",
    discoveryState: "managed",
    discoverySources: [candidate.discoverySource],
    confidence: candidate.confidence,
    pid: candidate.pid,
    tmuxSession: session,
    tmuxPane: candidate.tmuxPane,
    sshHost: candidate.sshHost,
    sshUser: candidate.sshUser,
    sshPort: candidate.sshPort,
    sshPasswordRequired: Boolean(candidate.sshPasswordRequired),
    startedAt: candidate.detectedAt,
    updatedAt: new Date().toISOString(),
    durationSec: 0,
    terminalTarget: buildTerminalTarget(candidate.location, "ghostty", candidate.cwd, session, candidate.sshHost, candidate.sshUser, candidate.sshPort || 22),
  };
}

function buildTerminalTarget(location, terminalApp, cwd, sessionName, sshHost, sshUser, sshPort) {
  if (location === "remote") {
    return {
      type: "ssh-tmux",
      terminalApp,
      sshHost,
      sshUser,
      sshPort: Number(sshPort || 22),
      sessionName,
      remoteCommand: `tmux attach -t ${sessionName} || tmux new -s ${sessionName}`,
    };
  }
  return {
    type: "local-shell",
    terminalApp,
    sessionName,
    localCommand: `cd ${cwd || "~"} && tmux attach -t ${sessionName} || tmux new -s ${sessionName}`,
  };
}

function render() {
  const running = state.agents.filter((agent) => agent.status === "running").length;
  const attention = state.agents.filter((agent) => agent.status === "waiting_attention").length;
  els.runningCount.textContent = running;
  els.attentionCount.textContent = attention;
  els.refreshBtn.textContent = state.scanning ? "扫描中" : "立即刷新";
  els.refreshBtn.disabled = state.scanning || refreshInFlight;
  els.refreshBtn.title = state.collectorOnline ? `自动监测中 · ${scanResultText()}` : "立即刷新检测";
  els.lastUpdated.textContent = `更新于 ${formatTime(state.lastUpdated)}`;

  els.agentList.innerHTML = "";
  if (state.agents.length === 0) {
    els.agentList.innerHTML = `<div class="empty-state">暂无已管理 Agent，点击右上角 + 手动添加。</div>`;
  } else {
    state.agents.forEach((agent) => els.agentList.appendChild(renderAgentCard(agent)));
  }

  renderCandidates();
}

function renderAgentCard(agent) {
  const card = document.createElement("article");
  const isEditingName = editingAgentId === agent.id;
  card.className = "agent-card";
  card.innerHTML = `
    <div class="agent-summary" role="button" tabindex="0" aria-expanded="false">
      <span class="agent-icon ${agent.kind} ${agent.status}" aria-hidden="true">${kindGlyph(agent.kind)}</span>
      <div class="agent-title">
        <div class="agent-name-row ${isEditingName ? "is-editing" : ""}">
          ${
            isEditingName
              ? `<input class="agent-name-input" value="${escapeHtml(editingAgentName)}" maxlength="80" aria-label="编辑 Agent 备注" />`
              : `<h3>${escapeHtml(agent.name)}</h3>`
          }
          <button class="name-edit-button ${isEditingName ? "is-confirm" : ""}" data-name-action="${isEditingName ? "save" : "edit"}" title="${isEditingName ? "保存备注" : "修改备注"}" aria-label="${isEditingName ? "保存备注" : "修改备注"}">${isEditingName ? "✓" : "✎"}</button>
          ${sshPasswordBadge(agent)}
        </div>
        <div class="agent-meta">${kindName(agent.kind)} · ${escapeHtml(agentContext(agent))}</div>
        <div class="agent-state-line ${agent.status}">${escapeHtml(agentStateLine(agent))}</div>
      </div>
      <div class="agent-status-stack">
        <span class="badge ${agent.status}">${statusName(agent.status)}</span>
        <button class="terminal-button ${agent.status}" data-open="${escapeHtml(agent.id)}"><span>›_</span>${openButtonText(agent)}</button>
      </div>
      <span class="agent-chevron" aria-hidden="true">⌄</span>
    </div>
    ${agentStatusBanner(agent)}
    <div class="agent-body">
      <div class="agent-details">
        ${agentDetail("▣", "机器", agent.machineLabel)}
        ${agentDetail("⌘", "路径", agent.cwd || "未设置路径")}
        ${agentDetail("♟", "当前任务", agent.currentTask || "等待状态上报")}
        ${agentDetail("⌘", "最新输出", agent.lastOutput || "暂无最近输出", "output")}
        ${agentDetail("◷", "运行时长", formatDuration(agent.durationSec))}
      </div>
      <div class="agent-actions">
        <div class="source-line">${escapeHtml(agent.location === "remote" ? "远程会话" : "本地会话")}</div>
      </div>
    </div>
  `;
  const summary = card.querySelector(".agent-summary");
  const toggleCard = () => {
    const expanded = card.classList.toggle("is-expanded");
    summary.setAttribute("aria-expanded", String(expanded));
  };
  const saveName = async () => {
    const input = card.querySelector(".agent-name-input");
    const name = (input?.value || editingAgentName || agent.name).trim();
    if (!name) return;
    const response = await invoke("rename_agent", { id: agent.id, name });
    if (response?.ok) {
      state = {
        ...state,
        agents: state.agents.map((item) => (item.id === agent.id ? { ...item, name } : item)),
      };
      editingAgentId = null;
      editingAgentName = "";
      render();
    }
  };
  summary.addEventListener("click", (event) => {
    if (event.target.closest("[data-open], [data-name-action], [data-ssh-password-agent], .agent-name-input")) return;
    toggleCard();
  });
  summary.addEventListener("keydown", (event) => {
    if (event.target.closest(".agent-name-input")) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    toggleCard();
  });
  card.querySelector("[data-name-action]").addEventListener("click", async (event) => {
    event.stopPropagation();
    if (editingAgentId === agent.id) {
      await saveName();
      return;
    }
    editingAgentId = agent.id;
    editingAgentName = agent.name;
    render();
  });
  card.querySelector("[data-ssh-password-agent]")?.addEventListener("click", (event) => {
    event.stopPropagation();
    openSshPasswordDialog(agent);
  });
  const nameInput = card.querySelector(".agent-name-input");
  if (nameInput) {
    nameInput.focus();
    nameInput.select();
    nameInput.addEventListener("input", () => {
      editingAgentName = nameInput.value;
    });
    nameInput.addEventListener("click", (event) => event.stopPropagation());
    nameInput.addEventListener("keydown", async (event) => {
      event.stopPropagation();
      if (event.key === "Enter") {
        event.preventDefault();
        await saveName();
      }
      if (event.key === "Escape") {
        editingAgentId = null;
        editingAgentName = "";
        render();
      }
    });
  }
  card.querySelector("[data-open]").addEventListener("click", async (event) => {
    event.stopPropagation();
    await invoke("open_terminal", { target: agent.terminalTarget });
  });
  return card;
}

function sshPasswordBadge(agent) {
  if (!agent.sshPasswordRequired || !agent.sshHost) return "";
  return `
    <button class="ssh-password-badge" data-ssh-password-agent="${escapeHtml(agent.id)}" title="保存这个远程连接的 SSH 密码" aria-label="保存 SSH 密码">
      需要 SSH 密码
    </button>
  `;
}

function openSshPasswordDialog(agent) {
  if (!els.sshPasswordDialog || !els.sshPasswordForm) return;
  const form = els.sshPasswordForm;
  form.reset();
  form.elements.sshHost.value = agent.sshHost || agent.terminalTarget?.sshHost || "";
  form.elements.sshUser.value = agent.sshUser || agent.terminalTarget?.sshUser || "";
  form.elements.sshPort.value = String(agent.sshPort || agent.terminalTarget?.sshPort || 22);
  const userPrefix = form.elements.sshUser.value ? `${form.elements.sshUser.value}@` : "";
  els.sshPasswordTarget.textContent = `${userPrefix}${form.elements.sshHost.value}:${form.elements.sshPort.value}`;
  els.sshPasswordDialog.showModal();
  setTimeout(() => form.elements.password?.focus(), 40);
}

function agentDetail(icon, label, value, tone = "") {
  return `
    <div class="agent-detail ${tone}">
      <span class="detail-icon" aria-hidden="true">${icon}</span>
      <span class="detail-label">${label}</span>
      <span class="detail-value">${escapeHtml(value)}</span>
    </div>
  `;
}

function renderCandidates() {
  els.candidateList.innerHTML = "";
  if (state.candidates.length === 0) {
    els.candidateList.innerHTML = `<div class="empty-state">没有待确认 Agent。</div>`;
    return;
  }
  state.candidates.forEach((candidate) => {
    const card = document.createElement("article");
    card.className = "candidate-card";
    card.innerHTML = `
      <h3>检测到 ${kindName(candidate.kind)}</h3>
      <div class="agent-meta">${escapeHtml(candidate.machineLabel)} · ${escapeHtml(candidate.cwd || candidate.command || "未知路径")}</div>
      <div class="agent-meta">来源：${escapeHtml(candidate.discoverySource)} · 置信度 ${Math.round((candidate.confidence || 0) * 100)}%</div>
      <div class="agent-actions" style="margin-top: 10px;">
        <button data-confirm="${escapeHtml(candidate.fingerprint)}">添加到管理</button>
        <button data-ignore="${escapeHtml(candidate.fingerprint)}">忽略</button>
      </div>
    `;
    card.querySelector("[data-confirm]").addEventListener("click", async () => {
      await invoke("confirm_candidate", { fingerprint: candidate.fingerprint, name: "" });
      await syncState();
    });
    card.querySelector("[data-ignore]").addEventListener("click", async () => {
      await invoke("ignore_candidate", { fingerprint: candidate.fingerprint });
      await syncState();
    });
    els.candidateList.appendChild(card);
  });
}

async function syncState() {
  try {
    const next = await invoke("get_state");
    if (next) state = next;
  } catch (error) {
    state = {
      ...state,
      collectorOnline: false,
      scanReports: [
        {
          source: "collector",
          label: "Collector",
          ok: false,
          message: "未连接桌面后端，正在显示演示数据",
          checkedAt: new Date().toISOString(),
        },
      ],
    };
  }
  render();
}

async function refreshDiscovery() {
  if (refreshInFlight) return;
  refreshInFlight = true;
  state = { ...state, scanning: true };
  render();
  try {
    lastScanResult = await invoke("discovery_scan", { scope: "all" });
    await syncState();
  } catch (error) {
    state = {
      ...state,
      scanning: false,
      collectorOnline: false,
      lastUpdated: new Date().toISOString(),
    };
  } finally {
    refreshInFlight = false;
    state = { ...state, scanning: false };
    render();
  }
}

function startAutoRefresh() {
  clearInterval(autoRefreshTimer);
  autoRefreshTimer = setInterval(refreshDiscovery, autoRefreshIntervalMs);
  setTimeout(refreshDiscovery, 800);
}

function scanResultText() {
  if (!lastScanResult) return `每 ${Math.round(autoRefreshIntervalMs / 1000)} 秒刷新`;
  const parts = [`检测 ${lastScanResult.detectedCount ?? 0}`];
  if (lastScanResult.newCount) parts.push(`新增 ${lastScanResult.newCount}`);
  if (lastScanResult.removedCount) parts.push(`移除 ${lastScanResult.removedCount}`);
  return parts.join(" · ");
}

function updateRemoteFields() {
  const isRemote = els.locationSelect.value === "remote";
  document.querySelectorAll(".remote-fields").forEach((field) => {
    field.classList.toggle("hidden", !isRemote);
  });
}

function readManualAgent(form) {
  const data = Object.fromEntries(new FormData(form).entries());
  const sessionName = data.sessionName || `${data.kind}_session`;
  const location = data.location || "local";
  const terminalTarget = buildTerminalTarget(
    location,
    data.terminalApp || "ghostty",
    data.cwd,
    sessionName,
    data.sshHost,
    data.sshUser,
    data.sshPort,
  );
  return {
    id: `manual-${Date.now()}`,
    name: data.name,
    kind: data.kind,
    location,
    machineLabel: data.machineLabel || (location === "remote" ? data.sshHost || "Remote Server" : "Local Mac"),
    cwd: data.cwd,
    currentTask: data.currentTask,
    lastOutput: "由用户手动添加。",
    status: "unknown",
    discoveryState: "managed",
    discoverySources: ["manual"],
    confidence: 1,
    tmuxSession: sessionName,
    sshHost: data.sshHost,
    sshUser: data.sshUser,
    sshPort: Number(data.sshPort || 22),
    sshPasswordRequired: false,
    updatedAt: new Date().toISOString(),
    durationSec: 0,
    terminalTarget,
  };
}

function kindName(kind) {
  return {
    codex: "Codex CLI",
    codex_desktop: "Codex Desktop",
    claude_code: "Claude Code",
    other: "Other",
    unknown: "Unknown",
  }[kind] || "Agent";
}

function kindGlyph(kind) {
  return {
    codex: "›_",
    codex_desktop: "⌘",
    claude_code: "⌘",
    other: "◇",
    unknown: "?",
  }[kind] || "›_";
}

function statusName(status) {
  return {
    running: "运行中",
    waiting_attention: "需关注",
    done: "已完成",
    error: "错误",
    offline: "离线",
    unknown: "未知",
  }[status] || "未知";
}

function agentStateLine(agent) {
  return {
    running: "最近扫描检测到 Agent 仍在运行",
    waiting_attention: "终端正在等待你的审批 / Yes / Enter",
    done: "Agent 已结束或终端已回到空闲 shell",
    error: "Agent 上报了错误状态",
    offline: "进程已离线，等待自动清理",
    unknown: "等待下一次状态信号",
  }[agent.status] || "等待下一次状态信号";
}

function agentStatusBanner(agent) {
  if (!["waiting_attention", "done", "error"].includes(agent.status)) return "";
  return `<div class="agent-status-banner ${agent.status}">${escapeHtml(agentStateLine(agent))}</div>`;
}

function agentContext(agent) {
  const parts = [(agent.discoverySources || []).join(" + ") || "manual"];
  if (agent.tmuxSession) parts.push(`tmux:${agent.tmuxSession}`);
  if (agent.pid) parts.push(`pid:${agent.pid}`);
  if (agent.terminalTarget?.tty) parts.push(agent.terminalTarget.tty);
  if (agent.terminalTarget?.terminalPid) parts.push(`term:${agent.terminalTarget.terminalPid}`);
  return parts.join(" · ");
}

function openButtonText(agent) {
  if (agent.terminalTarget?.terminalApp === "vscode") return "定位 VS Code";
  if (agent.terminalTarget?.type === "desktop-app") return "定位 Codex";
  if (agent.terminalTarget?.type === "local-process" && agent.terminalTarget?.tty) return "定位终端";
  if (agent.terminalTarget?.type === "local-process") return "查看进程";
  if (agent.terminalTarget?.type === "ssh-process") return "查看远程进程";
  return "打开终端";
}

function showPermissionGuide({ force = false } = {}) {
  if (!els.permissionDialog) return;
  if (!force && localStorage.getItem(permissionGuideStorageKey) === "done") return;
  if (els.permissionDialog.open) return;
  els.permissionDialog.showModal();
}

function closePermissionGuide({ remember = true } = {}) {
  if (remember) localStorage.setItem(permissionGuideStorageKey, "done");
  els.permissionDialog?.close();
}

async function openPermissionPane(pane) {
  await invoke("open_permission_settings", { pane });
}

function formatTime(value) {
  if (!value) return "未知";
  return new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(new Date(value));
}

function formatDuration(sec = 0) {
  const minutes = Math.floor(sec / 60);
  if (minutes < 60) return `${minutes} 分钟`;
  return `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟`;
}

function escapeHtml(value = "") {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

els.refreshBtn.addEventListener("click", refreshDiscovery);
els.settingsBtn.addEventListener("click", () => els.settingsDialog.showModal());
els.addBtn.addEventListener("click", () => els.manualDialog.showModal());
els.closeManualBtn.addEventListener("click", () => els.manualDialog.close());
els.closeSshPasswordBtn?.addEventListener("click", () => els.sshPasswordDialog?.close());
els.permissionGuideBtn?.addEventListener("click", () => showPermissionGuide({ force: true }));
els.skipPermissionBtn?.addEventListener("click", () => closePermissionGuide());
els.finishPermissionBtn?.addEventListener("click", () => closePermissionGuide());
document.querySelectorAll("[data-permission-open]").forEach((button) => {
  button.addEventListener("click", () => openPermissionPane(button.dataset.permissionOpen));
});
document.querySelectorAll("input[name='themeMode']").forEach((input) => {
  input.addEventListener("change", () => applyTheme(input.value));
});
els.locationSelect.addEventListener("change", updateRemoteFields);
els.manualForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const agent = readManualAgent(els.manualForm);
  await invoke("manual_agent", { agent });
  els.manualDialog.close();
  els.manualForm.reset();
  updateRemoteFields();
  await syncState();
});
els.sshPasswordForm?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const data = Object.fromEntries(new FormData(els.sshPasswordForm).entries());
  const response = await invoke("save_ssh_password", {
    sshHost: data.sshHost,
    sshUser: data.sshUser || null,
    sshPort: Number(data.sshPort || 22),
    password: data.password,
  });
  if (!response?.ok) return;
  els.sshPasswordDialog.close();
  els.sshPasswordForm.reset();
  await refreshDiscovery();
});

updateRemoteFields();
applyTheme();
setupWindowDragging();
render();
syncState();
startAutoRefresh();
setTimeout(() => showPermissionGuide(), 500);
