#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeskSnapshot {
    version: String,
    scanning: bool,
    collector_online: bool,
    collector_url: String,
    last_updated: String,
    agents: Vec<AgentItem>,
    candidates: Vec<DiscoveredAgent>,
    scan_reports: Vec<ScanReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeskConfig {
    app: AppConfig,
    discovery: DiscoveryConfig,
    remote_hosts: Vec<RemoteHost>,
    agents: Vec<AgentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppConfig {
    listen_host: String,
    listen_port: u16,
    offline_timeout_sec: u64,
    default_terminal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryConfig {
    enabled: bool,
    scan_on_startup: bool,
    local_scan_interval_sec: u64,
    remote_scan_interval_sec: u64,
    auto_add_mode: String,
    scan_local_processes: bool,
    scan_local_tmux: bool,
    scan_remote_hosts: bool,
    trusted_commands: Vec<String>,
    ignored_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteHost {
    id: String,
    label: String,
    ssh_host: String,
    ssh_user: String,
    ssh_port: u16,
    scan_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentItem {
    id: String,
    name: String,
    kind: AgentKind,
    location: AgentLocation,
    machine_label: String,
    project_label: Option<String>,
    cwd: Option<String>,
    current_task: Option<String>,
    last_output: Option<String>,
    status: AgentStatus,
    discovery_state: AgentDiscoveryState,
    discovery_sources: Vec<String>,
    confidence: Option<f32>,
    pid: Option<u32>,
    tmux_session: Option<String>,
    tmux_pane: Option<String>,
    ssh_host: Option<String>,
    ssh_user: Option<String>,
    started_at: Option<String>,
    updated_at: String,
    duration_sec: Option<u64>,
    terminal_target: Option<TerminalTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentKind {
    Codex,
    CodexDesktop,
    ClaudeCode,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentLocation {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentStatus {
    Running,
    WaitingAttention,
    Done,
    Error,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentDiscoveryState {
    Managed,
    Candidate,
    Ignored,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalTarget {
    #[serde(rename = "type")]
    target_type: String,
    terminal_app: String,
    process_pid: Option<u32>,
    terminal_pid: Option<u32>,
    tty: Option<String>,
    process_command: Option<String>,
    ssh_host: Option<String>,
    ssh_user: Option<String>,
    ssh_port: Option<u16>,
    session_name: Option<String>,
    local_command: Option<String>,
    remote_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveredAgent {
    fingerprint: String,
    agent_id: Option<String>,
    name: Option<String>,
    kind: AgentKind,
    location: AgentLocation,
    machine_label: String,
    cwd: Option<String>,
    command: Option<String>,
    last_output: Option<String>,
    status: Option<AgentStatus>,
    pid: Option<u32>,
    parent_pid: Option<u32>,
    terminal_pid: Option<u32>,
    tty: Option<String>,
    tmux_session: Option<String>,
    tmux_pane: Option<String>,
    ssh_host: Option<String>,
    ssh_user: Option<String>,
    discovery_source: String,
    confidence: f32,
    detected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanReport {
    source: String,
    label: String,
    ok: bool,
    message: String,
    checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentEvent {
    agent_id: Option<String>,
    name: Option<String>,
    kind: Option<AgentKind>,
    location: Option<AgentLocation>,
    machine_label: Option<String>,
    cwd: Option<String>,
    current_task: Option<String>,
    last_output: Option<String>,
    status: Option<AgentStatus>,
    terminal_target: Option<TerminalTarget>,
    tmux_session: Option<String>,
    tmux_pane: Option<String>,
    ssh_host: Option<String>,
    ssh_user: Option<String>,
    discovery_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiOk {
    ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanResponse {
    ok: bool,
    detected_count: usize,
    new_count: usize,
    removed_count: usize,
}

struct AppStore {
    snapshot: Mutex<DeskSnapshot>,
    config: Mutex<DeskConfig>,
}

const SCAN_DISCOVERY_SOURCES: [&str; 7] = [
    "local_process",
    "local_ssh",
    "local_ssh_tmux",
    "local_tmux",
    "codex_desktop",
    "remote_process",
    "remote_tmux",
];

fn main() {
    let config = load_or_create_config();
    let collector_url = format!(
        "http://{}:{}",
        config.app.listen_host, config.app.listen_port
    );
    let snapshot = DeskSnapshot {
        version: "0.1.0".to_string(),
        scanning: false,
        collector_online: false,
        collector_url,
        last_updated: now_iso(),
        agents: config.agents.clone(),
        candidates: Vec::new(),
        scan_reports: Vec::new(),
    };

    let store = Arc::new(AppStore {
        snapshot: Mutex::new(snapshot),
        config: Mutex::new(config),
    });
    let setup_store = store.clone();

    tauri::Builder::default()
        .manage(store)
        .invoke_handler(tauri::generate_handler![
            get_state,
            discovery_scan,
            get_candidates,
            confirm_candidate,
            ignore_candidate,
            manual_agent,
            rename_agent,
            post_event,
            open_terminal,
            open_config_file
        ])
        .setup(move |_app| {
            start_collector(setup_store.clone());
            start_background_scanner(setup_store.clone());

            let scan_on_startup = setup_store
                .config
                .lock()
                .map(|config| config.discovery.scan_on_startup)
                .unwrap_or(false);
            if scan_on_startup {
                let scan_store = setup_store.clone();
                thread::spawn(move || {
                    run_scan(&scan_store);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Agent Pilot");
}

#[tauri::command]
fn get_state(state: State<Arc<AppStore>>) -> DeskSnapshot {
    refresh_agent_durations(&state);
    state.snapshot.lock().unwrap().clone()
}

#[tauri::command]
fn discovery_scan(_scope: String, state: State<Arc<AppStore>>) -> ScanResponse {
    run_scan(&state)
}

#[tauri::command]
fn get_candidates(state: State<Arc<AppStore>>) -> Vec<DiscoveredAgent> {
    state.snapshot.lock().unwrap().candidates.clone()
}

#[tauri::command]
fn confirm_candidate(fingerprint: String, name: String, state: State<Arc<AppStore>>) -> ApiOk {
    ApiOk {
        ok: confirm_candidate_inner(&state, &fingerprint, &name).is_some(),
    }
}

#[tauri::command]
fn ignore_candidate(fingerprint: String, state: State<Arc<AppStore>>) -> ApiOk {
    ignore_candidate_inner(&state, &fingerprint)
}

#[tauri::command]
fn manual_agent(agent: AgentItem, state: State<Arc<AppStore>>) -> ApiOk {
    manual_agent_inner(&state, agent)
}

#[tauri::command]
fn rename_agent(id: String, name: String, state: State<Arc<AppStore>>) -> ApiOk {
    rename_agent_inner(&state, &id, &name)
}

#[tauri::command]
fn post_event(event: AgentEvent, state: State<Arc<AppStore>>) -> ApiOk {
    handle_event(&state, event)
}

#[tauri::command]
fn open_terminal(target: Option<TerminalTarget>) -> ApiOk {
    open_terminal_inner(target)
}

#[tauri::command]
fn open_config_file() -> ApiOk {
    let path = config_path();
    let _ = Command::new("open").arg(path).spawn();
    ApiOk { ok: true }
}

fn start_background_scanner(store: Arc<AppStore>) {
    thread::spawn(move || loop {
        let interval = store
            .config
            .lock()
            .map(|config| config.discovery.local_scan_interval_sec.max(5))
            .unwrap_or(15);
        thread::sleep(Duration::from_secs(interval));

        let enabled = store
            .config
            .lock()
            .map(|config| config.discovery.enabled)
            .unwrap_or(false);
        if enabled {
            run_scan(&store);
        }
    });
}

fn start_collector(store: Arc<AppStore>) {
    thread::spawn(move || {
        let (host, port) = store
            .config
            .lock()
            .map(|config| (config.app.listen_host.clone(), config.app.listen_port))
            .unwrap_or_else(|_| ("127.0.0.1".to_string(), 8787));
        let addr = format!("{host}:{port}");
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(error) => {
                push_scan_report(
                    &store,
                    ScanReport {
                        source: "collector".to_string(),
                        label: addr,
                        ok: false,
                        message: format!("Collector bind failed: {error}"),
                        checked_at: now_iso(),
                    },
                );
                return;
            }
        };

        if let Ok(mut snapshot) = store.snapshot.lock() {
            snapshot.collector_online = true;
            snapshot.last_updated = now_iso();
        }

        for stream in listener.incoming().flatten() {
            let request_store = store.clone();
            thread::spawn(move || {
                handle_http_stream(stream, request_store);
            });
        }
    });
}

fn handle_http_stream(mut stream: TcpStream, store: Arc<AppStore>) {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 8192];
    let mut header_end = None;
    let mut content_length = 0_usize;

    for _ in 0..8 {
        let Ok(read) = stream.read(&mut temp) else {
            return;
        };
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        if header_end.is_none() {
            header_end = find_header_end(&buffer);
            if let Some(end) = header_end {
                let header = String::from_utf8_lossy(&buffer[..end]);
                content_length = parse_content_length(&header);
            }
        }
        if let Some(end) = header_end {
            if buffer.len() >= end + 4 + content_length {
                break;
            }
        }
    }

    let Some(end) = find_header_end(&buffer) else {
        write_json_response(
            &mut stream,
            400,
            json!({ "ok": false, "error": "bad_request" }),
        );
        return;
    };
    let header = String::from_utf8_lossy(&buffer[..end]);
    let mut lines = header.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default();
    let body = String::from_utf8_lossy(&buffer[end + 4..]).to_string();

    if method == "OPTIONS" {
        write_json_response(&mut stream, 204, json!({}));
        return;
    }

    let (status, payload) = handle_http_route(method, path, &body, &store);
    write_json_response(&mut stream, status, payload);
}

fn handle_http_route(method: &str, path: &str, body: &str, store: &Arc<AppStore>) -> (u16, Value) {
    match (method, path) {
        ("GET", "/api/health") => (
            200,
            json!({
                "ok": true,
                "version": "0.1.0",
                "collectorUrl": store.snapshot.lock().map(|s| s.collector_url.clone()).unwrap_or_default()
            }),
        ),
        ("GET", "/api/state") => {
            refresh_agent_durations(store);
            let snapshot = store.snapshot.lock().unwrap().clone();
            (200, json!(snapshot))
        }
        ("GET", "/api/discovery/candidates") => {
            let candidates = store.snapshot.lock().unwrap().candidates.clone();
            (200, json!({ "candidates": candidates }))
        }
        ("POST", "/api/discovery/scan") => {
            let response = run_scan(store);
            (200, json!(response))
        }
        ("POST", "/api/discovery/confirm") => {
            let payload: Value = parse_body(body);
            let fingerprint = payload
                .get("fingerprint")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let agent = confirm_candidate_inner(store, fingerprint, name);
            (200, json!({ "ok": agent.is_some(), "agent": agent }))
        }
        ("POST", "/api/discovery/ignore") => {
            let payload: Value = parse_body(body);
            let fingerprint = payload
                .get("fingerprint")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (200, json!(ignore_candidate_inner(store, fingerprint)))
        }
        ("POST", "/api/agents/manual") => {
            let payload: Value = parse_body(body);
            let agent_value = payload.get("agent").cloned().unwrap_or(payload);
            match serde_json::from_value::<AgentItem>(agent_value) {
                Ok(agent) => (200, json!(manual_agent_inner(store, agent))),
                Err(error) => (400, json!({ "ok": false, "error": error.to_string() })),
            }
        }
        ("POST", "/api/agents/rename") => {
            let payload: Value = parse_body(body);
            let id = payload
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (200, json!(rename_agent_inner(store, id, name)))
        }
        ("POST", "/api/events") => match serde_json::from_str::<AgentEvent>(body) {
            Ok(event) => (200, json!(handle_event(store, event))),
            Err(error) => (400, json!({ "ok": false, "error": error.to_string() })),
        },
        ("POST", "/api/open-terminal") => {
            let payload: Value = parse_body(body);
            let target_value = payload.get("target").cloned().unwrap_or(payload);
            let target = serde_json::from_value::<TerminalTarget>(target_value).ok();
            (200, json!(open_terminal_inner(target)))
        }
        _ => (404, json!({ "ok": false, "error": "not_found" })),
    }
}

fn run_scan(store: &Arc<AppStore>) -> ScanResponse {
    {
        let mut snapshot = store.snapshot.lock().unwrap();
        snapshot.scanning = true;
        snapshot.last_updated = now_iso();
    }

    let config = store.config.lock().unwrap().clone();
    let mut detected = Vec::new();
    let mut reports = Vec::new();

    if config.discovery.enabled && config.discovery.scan_local_processes {
        let (items, report) = scan_local_processes();
        detected.extend(items);
        reports.push(report);
        let (items, report) = scan_codex_desktop();
        detected.extend(items);
        reports.push(report);
    }
    if config.discovery.enabled && config.discovery.scan_local_tmux {
        let (items, report) = scan_local_tmux();
        detected.extend(items);
        reports.push(report);
    }
    if config.discovery.enabled && config.discovery.scan_remote_hosts {
        for host in config.remote_hosts.iter().filter(|host| host.scan_enabled) {
            let (process_items, process_report) = scan_remote_processes(host);
            let (tmux_items, tmux_report) = scan_remote_tmux(host);
            detected.extend(process_items);
            detected.extend(tmux_items);
            reports.push(process_report);
            reports.push(tmux_report);
        }
    }

    let detected = dedup(detected);
    let detected_count = detected.len();
    let detected_fingerprints: HashSet<String> = detected
        .iter()
        .map(|candidate| candidate.fingerprint.clone())
        .collect();
    let ignored: HashSet<String> = config.discovery.ignored_fingerprints.into_iter().collect();
    let mut snapshot = store.snapshot.lock().unwrap();
    let removed_count =
        prune_inactive_discovered_agents(&mut snapshot.agents, &detected_fingerprints);
    snapshot.candidates.retain(|candidate| {
        !is_scan_discovery_source(&candidate.discovery_source)
            || detected_fingerprints.contains(&candidate.fingerprint)
    });

    let known: HashSet<String> = snapshot
        .agents
        .iter()
        .flat_map(agent_fingerprints)
        .collect();
    let mut known = known;

    let mut new_count = 0;
    for candidate in detected {
        if ignored.contains(&candidate.fingerprint) {
            continue;
        }

        if known.contains(&candidate.fingerprint) {
            update_managed_from_candidate(&mut snapshot.agents, &candidate);
            continue;
        }

        if let Some(index) = snapshot
            .candidates
            .iter()
            .position(|item| item.fingerprint == candidate.fingerprint)
        {
            snapshot.candidates.remove(index);
        }

        let agent = agent_from_candidate(candidate, String::new());
        known.extend(agent_fingerprints(&agent));
        snapshot.agents.insert(0, agent);
        new_count += 1;
    }

    snapshot.scan_reports = reports;
    snapshot.scanning = false;
    snapshot.last_updated = now_iso();
    persist_agents(store, snapshot.agents.clone());

    ScanResponse {
        ok: true,
        detected_count,
        new_count,
        removed_count,
    }
}

fn confirm_candidate_inner(
    store: &Arc<AppStore>,
    fingerprint: &str,
    name: &str,
) -> Option<AgentItem> {
    let mut snapshot = store.snapshot.lock().unwrap();
    let index = snapshot
        .candidates
        .iter()
        .position(|candidate| candidate.fingerprint == fingerprint)?;
    let candidate = snapshot.candidates.remove(index);
    let agent = agent_from_candidate(candidate, name.to_string());
    snapshot.agents.insert(0, agent.clone());
    snapshot.last_updated = now_iso();
    persist_agents(store, snapshot.agents.clone());
    Some(agent)
}

fn ignore_candidate_inner(store: &Arc<AppStore>, fingerprint: &str) -> ApiOk {
    let mut snapshot = store.snapshot.lock().unwrap();
    snapshot
        .candidates
        .retain(|candidate| candidate.fingerprint != fingerprint);
    snapshot.last_updated = now_iso();

    if let Ok(mut config) = store.config.lock() {
        if !config
            .discovery
            .ignored_fingerprints
            .iter()
            .any(|item| item == fingerprint)
        {
            config
                .discovery
                .ignored_fingerprints
                .push(fingerprint.to_string());
            save_config(&config);
        }
    }
    ApiOk { ok: true }
}

fn manual_agent_inner(store: &Arc<AppStore>, mut agent: AgentItem) -> ApiOk {
    if agent.id.trim().is_empty() {
        agent.id = format!("manual-{}", now_millis());
    }
    if agent.updated_at.trim().is_empty() {
        agent.updated_at = now_iso();
    }
    if agent.started_at.is_none() {
        agent.started_at = Some(now_iso());
    }

    let mut snapshot = store.snapshot.lock().unwrap();
    if let Some(index) = snapshot.agents.iter().position(|item| item.id == agent.id) {
        snapshot.agents[index] = agent;
    } else {
        snapshot.agents.insert(0, agent);
    }
    snapshot.last_updated = now_iso();
    persist_agents(store, snapshot.agents.clone());
    ApiOk { ok: true }
}

fn rename_agent_inner(store: &Arc<AppStore>, id: &str, name: &str) -> ApiOk {
    let name = name.trim();
    if id.trim().is_empty() || name.is_empty() {
        return ApiOk { ok: false };
    }

    let mut snapshot = store.snapshot.lock().unwrap();
    let Some(agent) = snapshot.agents.iter_mut().find(|agent| agent.id == id) else {
        return ApiOk { ok: false };
    };
    agent.name = name.to_string();
    agent.updated_at = now_iso();
    snapshot.last_updated = now_iso();
    persist_agents(store, snapshot.agents.clone());
    ApiOk { ok: true }
}

fn handle_event(store: &Arc<AppStore>, event: AgentEvent) -> ApiOk {
    let agent_id = event
        .agent_id
        .clone()
        .unwrap_or_else(|| format!("event-{}", now_millis()));
    let now = now_iso();
    let mut snapshot = store.snapshot.lock().unwrap();

    if let Some(agent) = snapshot
        .agents
        .iter_mut()
        .find(|agent| agent.id == agent_id)
    {
        if let Some(name) = event.name {
            agent.name = name;
        }
        if let Some(task) = event.current_task {
            agent.current_task = Some(task);
        }
        if let Some(output) = event.last_output {
            agent.last_output = Some(output);
        }
        if let Some(status) = event.status {
            agent.status = status;
        }
        if let Some(target) = event.terminal_target {
            agent.terminal_target = Some(target);
        }
        if let Some(session) = event.tmux_session {
            agent.tmux_session = Some(session);
        }
        agent.updated_at = now.clone();
        if !agent
            .discovery_sources
            .iter()
            .any(|source| source == "hook_report")
        {
            agent.discovery_sources.push("hook_report".to_string());
        }
        snapshot.last_updated = now;
        persist_agents(store, snapshot.agents.clone());
        return ApiOk { ok: true };
    }

    let AgentEvent {
        agent_id: _,
        name,
        kind,
        location,
        machine_label,
        cwd,
        current_task,
        last_output,
        status,
        terminal_target,
        tmux_session,
        tmux_pane,
        ssh_host,
        ssh_user,
        discovery_source,
    } = event;

    let kind = kind.unwrap_or(AgentKind::Unknown);
    let location = location.unwrap_or(AgentLocation::Local);
    let source = discovery_source.unwrap_or_else(|| "hook_report".to_string());
    let machine_label = machine_label.unwrap_or_else(|| match &location {
        AgentLocation::Local => "Local Mac".to_string(),
        AgentLocation::Remote => ssh_host
            .clone()
            .unwrap_or_else(|| "Remote Server".to_string()),
    });
    let session = tmux_session
        .clone()
        .unwrap_or_else(|| format!("{}_session", kind_key(&kind)));
    let terminal_target = terminal_target.or_else(|| {
        Some(build_terminal_target(
            &location,
            "ghostty",
            cwd.clone(),
            Some(session.clone()),
            ssh_host.clone(),
            ssh_user.clone(),
            Some(22),
        ))
    });

    let complete_for_auto_add = name.is_some() && cwd.is_some();
    let agent = AgentItem {
        id: agent_id,
        name: name.unwrap_or_else(|| format!("{} · {}", kind_label(&kind), machine_label)),
        kind,
        location,
        machine_label,
        project_label: None,
        cwd,
        current_task,
        last_output,
        status: status.unwrap_or(AgentStatus::Running),
        discovery_state: AgentDiscoveryState::Managed,
        discovery_sources: vec![source],
        confidence: Some(1.0),
        pid: None,
        tmux_session: Some(session),
        tmux_pane,
        ssh_host,
        ssh_user,
        started_at: Some(now.clone()),
        updated_at: now.clone(),
        duration_sec: Some(0),
        terminal_target,
    };

    if complete_for_auto_add {
        snapshot.agents.insert(0, agent);
        persist_agents(store, snapshot.agents.clone());
    } else {
        snapshot.candidates.insert(
            0,
            DiscoveredAgent {
                fingerprint: format!("hook:{}", agent.id),
                agent_id: Some(agent.id),
                name: Some(agent.name),
                kind: agent.kind,
                location: agent.location,
                machine_label: agent.machine_label,
                cwd: agent.cwd,
                command: agent.last_output,
                last_output: None,
                status: None,
                pid: None,
                parent_pid: None,
                terminal_pid: None,
                tty: None,
                tmux_session: agent.tmux_session,
                tmux_pane: agent.tmux_pane,
                ssh_host: agent.ssh_host,
                ssh_user: agent.ssh_user,
                discovery_source: "hook_report".to_string(),
                confidence: 0.95,
                detected_at: now.clone(),
            },
        );
    }
    snapshot.last_updated = now;
    ApiOk { ok: true }
}

fn open_terminal_inner(target: Option<TerminalTarget>) -> ApiOk {
    let Some(target) = target else {
        return ApiOk { ok: false };
    };
    if target.target_type == "desktop-app" {
        return ApiOk {
            ok: focus_desktop_app(&target),
        };
    }
    if target.target_type == "local-process" {
        return ApiOk {
            ok: open_local_process_terminal(&target),
        };
    }

    let command = terminal_command(&target);
    if command.is_empty() {
        return ApiOk { ok: false };
    }

    open_command_in_terminal(&target.terminal_app, &command);
    ApiOk { ok: true }
}

fn open_command_in_terminal(terminal_app: &str, command: &str) {
    match terminal_app {
        "terminal" => {
            let script = format!(
                "tell application \"Terminal\" to do script {}",
                apple_quote(&command)
            );
            let _ = Command::new("osascript").args(["-e", &script]).spawn();
        }
        "iterm" => {
            let script = format!(
                "tell application \"iTerm\" to create window with default profile command {}",
                apple_quote(&command)
            );
            let _ = Command::new("osascript").args(["-e", &script]).spawn();
        }
        _ => {
            let _ = Command::new("open")
                .args([
                    "-n", "-a", "Ghostty", "--args", "-e", "zsh", "-lc", &command,
                ])
                .spawn();
        }
    }
}

fn scan_local_processes() -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,tty=,command="])
        .output();
    let Ok(output) = output else {
        return (
            Vec::new(),
            ScanReport {
                source: "local_process".to_string(),
                label: "Local Mac".to_string(),
                ok: false,
                message: "ps command unavailable".to_string(),
                checked_at,
            },
        );
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let items: Vec<DiscoveredAgent> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let mut pieces = trimmed.split_whitespace();
            let pid_text = pieces.next()?;
            let parent_pid_text = pieces.next()?;
            let tty_text = pieces.next()?;
            let command = pieces.collect::<Vec<_>>().join(" ");
            let pid = pid_text.trim().parse::<u32>().ok()?;
            let parent_pid = parent_pid_text.trim().parse::<u32>().ok();
            if tty_text == "??" {
                return None;
            }
            let terminal_pid = find_terminal_parent_pid(parent_pid);
            let tty = Some(tty_text.to_string());
            if let Some(candidate) = infer_local_ssh_candidate(
                &command,
                pid,
                parent_pid,
                terminal_pid,
                tty.clone(),
                &checked_at,
            ) {
                return Some(candidate);
            }
            let kind = infer_process_kind(&command)?;
            let cwd = infer_cwd(&command);
            let confidence = if cwd.is_some() { 0.82 } else { 0.72 };
            let fingerprint = format!("local:{}:{}", kind_key(&kind), pid);
            let (status, last_output) = tty
                .as_deref()
                .and_then(capture_terminal_tab_text_by_tty)
                .map(|output| infer_status_from_terminal_text(&output, Some(&command)))
                .unwrap_or((None, None));
            Some(DiscoveredAgent {
                fingerprint,
                agent_id: None,
                name: None,
                kind,
                location: AgentLocation::Local,
                machine_label: "Local Mac".to_string(),
                cwd,
                command: Some(command),
                last_output,
                status,
                pid: Some(pid),
                parent_pid,
                terminal_pid,
                tty,
                tmux_session: None,
                tmux_pane: None,
                ssh_host: None,
                ssh_user: None,
                discovery_source: "local_process".to_string(),
                confidence,
                detected_at: checked_at.clone(),
            })
        })
        .collect();
    let count = items.len();
    (
        items,
        ScanReport {
            source: "local_process".to_string(),
            label: "Local Mac".to_string(),
            ok: true,
            message: format!("found {count} process candidates"),
            checked_at,
        },
    )
}

fn scan_local_tmux() -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}",
        ])
        .output();
    let Ok(output) = output else {
        return (
            Vec::new(),
            ScanReport {
                source: "local_tmux".to_string(),
                label: "Local Mac".to_string(),
                ok: false,
                message: "tmux command unavailable".to_string(),
                checked_at,
            },
        );
    };
    if !output.status.success() {
        return (
            Vec::new(),
            ScanReport {
                source: "local_tmux".to_string(),
                label: "Local Mac".to_string(),
                ok: true,
                message: "no tmux server".to_string(),
                checked_at,
            },
        );
    }
    let mut items = parse_tmux_lines(
        &String::from_utf8_lossy(&output.stdout),
        AgentLocation::Local,
        "Local Mac",
        None,
        None,
        &checked_at,
    );
    enrich_local_tmux_status(&mut items);
    let count = items.len();
    (
        items,
        ScanReport {
            source: "local_tmux".to_string(),
            label: "Local Mac".to_string(),
            ok: true,
            message: format!("found {count} tmux candidates"),
            checked_at,
        },
    )
}

fn scan_codex_desktop() -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let output = Command::new("ps").args(["-axo", "pid=,command="]).output();
    let Ok(output) = output else {
        return (
            Vec::new(),
            ScanReport {
                source: "codex_desktop".to_string(),
                label: "Codex Desktop".to_string(),
                ok: false,
                message: "ps command unavailable".to_string(),
                checked_at,
            },
        );
    };

    let mut items = Vec::new();
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        let Some((pid_text, command)) = trimmed.split_once(' ') else {
            continue;
        };
        let command = command.trim();
        if !is_codex_desktop_main_process(command) {
            continue;
        }
        let Some(pid) = pid_text.trim().parse::<u32>().ok() else {
            continue;
        };
        let (status, last_output) = codex_desktop_status_from_logs();
        items.push(DiscoveredAgent {
            fingerprint: format!("local:{}:{pid}", kind_key(&AgentKind::CodexDesktop)),
            agent_id: None,
            name: Some("Codex Desktop · Local Mac".to_string()),
            kind: AgentKind::CodexDesktop,
            location: AgentLocation::Local,
            machine_label: "Local Mac".to_string(),
            cwd: Some("Codex Desktop".to_string()),
            command: Some(command.to_string()),
            last_output,
            status,
            pid: Some(pid),
            parent_pid: None,
            terminal_pid: Some(pid),
            tty: None,
            tmux_session: None,
            tmux_pane: None,
            ssh_host: None,
            ssh_user: None,
            discovery_source: "codex_desktop".to_string(),
            confidence: 0.9,
            detected_at: checked_at.clone(),
        });
    }

    let count = items.len();
    (
        items,
        ScanReport {
            source: "codex_desktop".to_string(),
            label: "Codex Desktop".to_string(),
            ok: true,
            message: format!("found {count} Codex Desktop candidates"),
            checked_at,
        },
    )
}

fn is_codex_desktop_main_process(command: &str) -> bool {
    let lowered = command.to_lowercase();
    lowered.contains("/applications/codex.app/contents/macos/codex")
        && !lowered.contains("helper")
        && !lowered.contains("crashpad")
}

fn codex_desktop_status_from_logs() -> (Option<AgentStatus>, Option<String>) {
    let Some(logs_path) = home_path(".codex/logs_2.sqlite") else {
        return (
            Some(AgentStatus::Running),
            Some("Codex Desktop 已启动；未找到本地日志库。".to_string()),
        );
    };
    if !logs_path.exists() {
        return (
            Some(AgentStatus::Running),
            Some("Codex Desktop 已启动；暂无本地日志。".to_string()),
        );
    }

    let query = "select ts || char(9) || coalesce(feedback_log_body,'') from logs where target='codex_app_server::outgoing_message' and feedback_log_body like 'app-server event:%' order by ts desc, ts_nanos desc limit 80;";
    let output = Command::new("sqlite3").arg(logs_path).arg(query).output();
    let Ok(output) = output else {
        return (
            Some(AgentStatus::Running),
            Some("Codex Desktop 已启动；sqlite3 不可用，无法读取状态。".to_string()),
        );
    };
    if !output.status.success() {
        return (
            Some(AgentStatus::Running),
            Some("Codex Desktop 已启动；状态库读取失败。".to_string()),
        );
    }

    let now_sec = now_millis() / 1000;
    let mut latest_event = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((ts_text, body)) = line.split_once('\t') else {
            continue;
        };
        let ts = ts_text.trim().parse::<u64>().unwrap_or(0);
        let age_sec = now_sec.saturating_sub(ts);
        if age_sec > 900 {
            continue;
        }
        let method = codex_app_server_event_method(body);
        if latest_event.is_none() {
            latest_event = method.clone();
        }
        if let Some(method) = method {
            if is_codex_attention_event(&method) {
                return (
                    Some(AgentStatus::WaitingAttention),
                    Some(format!("Codex Desktop 等待处理：{method}")),
                );
            }
            if is_codex_completion_event(&method) {
                break;
            }
        }
    }

    (
        Some(AgentStatus::Running),
        Some(
            latest_event
                .map(|event| format!("Codex Desktop 最近事件：{event}"))
                .unwrap_or_else(|| "Codex Desktop 已启动；暂无最近任务事件。".to_string()),
        ),
    )
}

fn codex_app_server_event_method(body: &str) -> Option<String> {
    body.strip_prefix("app-server event: ")
        .and_then(|value| value.split_whitespace().next())
        .map(str::to_string)
}

fn is_codex_attention_event(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "mcpServer/elicitation/request"
    )
}

fn is_codex_completion_event(method: &str) -> bool {
    matches!(
        method,
        "turn/completed" | "serverRequest/resolved" | "thread/closed"
    )
}

fn scan_remote_processes(host: &RemoteHost) -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let target = format!("{}@{}", host.ssh_user, host.ssh_host);
    let remote = "pgrep -af '(claude|codex)' || true";
    let output = Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=2",
            "-o",
            "BatchMode=yes",
            "-p",
            &host.ssh_port.to_string(),
            &target,
            remote,
        ])
        .output();
    let Ok(output) = output else {
        return (
            Vec::new(),
            ScanReport {
                source: "remote_process".to_string(),
                label: host.label.clone(),
                ok: false,
                message: "ssh command failed".to_string(),
                checked_at,
            },
        );
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let items: Vec<DiscoveredAgent> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let (pid_text, command) = trimmed.split_once(' ')?;
            let pid = pid_text.trim().parse::<u32>().ok()?;
            let kind = infer_process_kind(command)?;
            let cwd = infer_cwd(command);
            let fingerprint = format!(
                "remote:{}@{}:{}:{}:{}",
                host.ssh_user,
                host.ssh_host,
                kind_key(&kind),
                pid,
                command
            );
            Some(DiscoveredAgent {
                fingerprint,
                agent_id: None,
                name: None,
                kind,
                location: AgentLocation::Remote,
                machine_label: host.label.clone(),
                cwd,
                command: Some(command.to_string()),
                last_output: None,
                status: None,
                pid: Some(pid),
                parent_pid: None,
                terminal_pid: None,
                tty: None,
                tmux_session: None,
                tmux_pane: None,
                ssh_host: Some(host.ssh_host.clone()),
                ssh_user: Some(host.ssh_user.clone()),
                discovery_source: "remote_process".to_string(),
                confidence: 0.68,
                detected_at: checked_at.clone(),
            })
        })
        .collect();
    let count = items.len();
    (
        items,
        ScanReport {
            source: "remote_process".to_string(),
            label: host.label.clone(),
            ok: output.status.success(),
            message: format!("found {count} remote process candidates"),
            checked_at,
        },
    )
}

fn scan_remote_tmux(host: &RemoteHost) -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let target = format!("{}@{}", host.ssh_user, host.ssh_host);
    let remote = "tmux list-panes -a -F '#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}' || true";
    let output = Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=2",
            "-o",
            "BatchMode=yes",
            "-p",
            &host.ssh_port.to_string(),
            &target,
            remote,
        ])
        .output();
    let Ok(output) = output else {
        return (
            Vec::new(),
            ScanReport {
                source: "remote_tmux".to_string(),
                label: host.label.clone(),
                ok: false,
                message: "ssh command failed".to_string(),
                checked_at,
            },
        );
    };
    let mut items = parse_tmux_lines(
        &String::from_utf8_lossy(&output.stdout),
        AgentLocation::Remote,
        &host.label,
        Some(&host.ssh_host),
        Some(&host.ssh_user),
        &checked_at,
    );
    enrich_remote_tmux_status(&mut items, host);
    let count = items.len();
    (
        items,
        ScanReport {
            source: "remote_tmux".to_string(),
            label: host.label.clone(),
            ok: output.status.success(),
            message: format!("found {count} remote tmux candidates"),
            checked_at,
        },
    )
}

fn parse_tmux_lines(
    text: &str,
    location: AgentLocation,
    machine_label: &str,
    ssh_host: Option<&String>,
    ssh_user: Option<&String>,
    checked_at: &str,
) -> Vec<DiscoveredAgent> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() != 4 {
                return None;
            }
            let session = parts[0].to_string();
            let pane = parts[1].to_string();
            let command = parts[2].to_string();
            let cwd = parts[3].to_string();
            let kind = infer_kind(&format!("{session} {command}"))?;
            let status = if is_idle_shell_command(&command) {
                Some(AgentStatus::Done)
            } else {
                None
            };
            let confidence = if command == "claude" || command == "codex" {
                0.9
            } else {
                0.82
            };
            let fingerprint = match &location {
                AgentLocation::Local => format!(
                    "local-tmux:{}:{}:{}:{}",
                    kind_key(&kind),
                    session,
                    pane,
                    cwd
                ),
                AgentLocation::Remote => format!(
                    "remote-tmux:{}@{}:{}:{}:{}:{}",
                    ssh_user.cloned().unwrap_or_default(),
                    ssh_host.cloned().unwrap_or_default(),
                    kind_key(&kind),
                    session,
                    pane,
                    cwd
                ),
            };
            Some(DiscoveredAgent {
                fingerprint,
                agent_id: None,
                name: None,
                kind,
                location: location.clone(),
                machine_label: machine_label.to_string(),
                cwd: if cwd.is_empty() { None } else { Some(cwd) },
                command: Some(command),
                last_output: None,
                status,
                pid: None,
                parent_pid: None,
                terminal_pid: None,
                tty: None,
                tmux_session: Some(session),
                tmux_pane: Some(pane),
                ssh_host: ssh_host.cloned(),
                ssh_user: ssh_user.cloned(),
                discovery_source: match &location {
                    AgentLocation::Local => "local_tmux",
                    AgentLocation::Remote => "remote_tmux",
                }
                .to_string(),
                confidence,
                detected_at: checked_at.to_string(),
            })
        })
        .collect()
}

fn enrich_local_tmux_status(items: &mut [DiscoveredAgent]) {
    for item in items {
        let Some(session) = item.tmux_session.as_deref() else {
            continue;
        };
        let Some(pane) = item.tmux_pane.as_deref() else {
            continue;
        };
        if let Some(output) = capture_local_tmux_pane(session, pane) {
            apply_terminal_output_hint(item, &output);
        }
    }
}

fn enrich_remote_tmux_status(items: &mut [DiscoveredAgent], host: &RemoteHost) {
    for item in items {
        let Some(session) = item.tmux_session.as_deref() else {
            continue;
        };
        let Some(pane) = item.tmux_pane.as_deref() else {
            continue;
        };
        if let Some(output) = capture_remote_tmux_pane(host, session, pane) {
            apply_terminal_output_hint(item, &output);
        }
    }
}

fn apply_terminal_output_hint(item: &mut DiscoveredAgent, output: &str) {
    let (status, last_output) = infer_status_from_terminal_text(output, item.command.as_deref());
    if status.is_some() {
        item.status = status;
    }
    if last_output.is_some() {
        item.last_output = last_output;
    }
}

fn capture_local_tmux_pane(session: &str, pane: &str) -> Option<String> {
    let target = format!("{session}:{pane}");
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-S", "-80", "-t", &target])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    non_empty_output(output.stdout)
}

fn capture_remote_tmux_pane(host: &RemoteHost, session: &str, pane: &str) -> Option<String> {
    let target = format!("{session}:{pane}");
    capture_ssh_tmux_output(Some(&host.ssh_user), &host.ssh_host, host.ssh_port, &target)
}

fn capture_ssh_tmux_output(
    ssh_user: Option<&String>,
    ssh_host: &str,
    ssh_port: u16,
    target: &str,
) -> Option<String> {
    let remote = format!(
        "tmux capture-pane -p -S -80 -t {} || true",
        shell_quote(target)
    );
    let ssh_target = ssh_user
        .map(|user| format!("{user}@{ssh_host}"))
        .unwrap_or_else(|| ssh_host.to_string());
    let output = Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=2",
            "-o",
            "BatchMode=yes",
            "-p",
            &ssh_port.to_string(),
            &ssh_target,
            &remote,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    non_empty_output(output.stdout)
}

fn capture_terminal_tab_text_by_tty(tty: &str) -> Option<String> {
    let short_tty = tty.trim_start_matches("/dev/");
    let full_tty = format!("/dev/{short_tty}");
    let script = format!(
        r#"tell application "Terminal"
  repeat with terminalWindowIndex from 1 to count of windows
    set previousTab to selected tab of window terminalWindowIndex
    repeat with terminalTabIndex from 1 to count of tabs of window terminalWindowIndex
      try
        set tabTty to tty of tab terminalTabIndex of window terminalWindowIndex as text
        if tabTty is {full_tty} or tabTty is {short_tty} then
          set terminalContents to contents of tab terminalTabIndex of window terminalWindowIndex as text
          set terminalText to terminalContents
          set selected tab of window terminalWindowIndex to tab terminalTabIndex of window terminalWindowIndex
          try
            tell application "System Events"
              tell process "Terminal"
                set accessibilityText to value of text area 1 of scroll area 1 of splitter group 1 of window terminalWindowIndex as text
              end tell
            end tell
            if terminalContents is "" and accessibilityText is not "" then
              set terminalText to accessibilityText
            end if
          end try
          set selected tab of window terminalWindowIndex to previousTab
          return terminalText
        end if
      end try
    end repeat
  end repeat
end tell
return "" "#,
        full_tty = apple_quote(&full_tty),
        short_tty = apple_quote(short_tty)
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    non_empty_output(output.stdout)
}

fn infer_status_from_terminal_text(
    text: &str,
    current_command: Option<&str>,
) -> (Option<AgentStatus>, Option<String>) {
    let excerpt = terminal_output_excerpt(text);
    let recent_text = excerpt.as_deref().unwrap_or(text);
    let status = if contains_attention_prompt(recent_text) {
        Some(AgentStatus::WaitingAttention)
    } else if current_command.map(is_idle_shell_command).unwrap_or(false) {
        Some(AgentStatus::Done)
    } else {
        None
    };
    (status, excerpt)
}

fn contains_attention_prompt(text: &str) -> bool {
    let lowered = text.to_lowercase();
    [
        "do you want to proceed",
        "yes, and don't ask again",
        "esc to cancel",
        "tab to amend",
        "ctrl+e to explain",
        "requires approval",
        "approval required",
        "permission required",
        "allow this command",
        "run this command",
        "approve",
        "proceed?",
        "continue?",
        "are you sure",
        "yes/no",
        "y/n",
        "是否继续",
        "需要审批",
        "需要批准",
        "确认执行",
        "允许执行",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn is_idle_shell_command(command: &str) -> bool {
    let command = command
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .rsplit('/')
        .next()
        .unwrap_or(command)
        .to_lowercase();
    matches!(
        command.as_str(),
        "sh" | "bash" | "zsh" | "fish" | "nu" | "pwsh" | "powershell" | "login"
    )
}

fn terminal_output_excerpt(text: &str) -> Option<String> {
    let lines = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .rev()
        .take(8)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let mut ordered = lines;
    ordered.reverse();
    let excerpt = ordered.join("\n");
    Some(
        excerpt
            .chars()
            .rev()
            .take(1200)
            .collect::<String>()
            .chars()
            .rev()
            .collect(),
    )
}

fn non_empty_output(stdout: Vec<u8>) -> Option<String> {
    let text = String::from_utf8_lossy(&stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn update_managed_from_candidate(agents: &mut [AgentItem], candidate: &DiscoveredAgent) {
    for agent in agents.iter_mut() {
        let fingerprints = agent_fingerprints(agent);
        if fingerprints
            .iter()
            .any(|item| item == &candidate.fingerprint)
        {
            agent.status = candidate.status.clone().unwrap_or(AgentStatus::Running);
            agent.updated_at = now_iso();
            agent.machine_label = candidate.machine_label.clone();
            agent.pid = candidate.pid.or(agent.pid);
            agent.tmux_session = candidate
                .tmux_session
                .clone()
                .or(agent.tmux_session.clone());
            agent.tmux_pane = candidate.tmux_pane.clone().or(agent.tmux_pane.clone());
            agent.cwd = candidate.cwd.clone().or(agent.cwd.clone());
            agent.last_output = candidate
                .last_output
                .clone()
                .or(candidate.command.clone())
                .or(agent.last_output.clone());
            agent.ssh_host = candidate.ssh_host.clone().or(agent.ssh_host.clone());
            agent.ssh_user = candidate.ssh_user.clone().or(agent.ssh_user.clone());
            if candidate.discovery_source == "local_ssh" {
                agent
                    .discovery_sources
                    .retain(|source| source != "local_ssh_tmux");
            }
            agent.terminal_target = Some(if should_use_process_terminal_target(candidate) {
                build_process_terminal_target(candidate)
            } else if candidate.discovery_source == "codex_desktop" {
                build_desktop_app_target(candidate)
            } else {
                build_terminal_target(
                    &candidate.location,
                    agent
                        .terminal_target
                        .as_ref()
                        .map(|target| target.terminal_app.as_str())
                        .unwrap_or("ghostty"),
                    candidate.cwd.clone().or(agent.cwd.clone()),
                    candidate
                        .tmux_session
                        .clone()
                        .or(agent.tmux_session.clone()),
                    candidate.ssh_host.clone().or(agent.ssh_host.clone()),
                    candidate.ssh_user.clone().or(agent.ssh_user.clone()),
                    Some(22),
                )
            });
            if !agent
                .discovery_sources
                .iter()
                .any(|source| source == &candidate.discovery_source)
            {
                agent
                    .discovery_sources
                    .push(candidate.discovery_source.clone());
            }
            return;
        }
    }
}

fn prune_inactive_discovered_agents(
    agents: &mut Vec<AgentItem>,
    detected_fingerprints: &HashSet<String>,
) -> usize {
    let before = agents.len();
    agents.retain(|agent| {
        if !is_scan_discovered_agent(agent) {
            return true;
        }
        agent_fingerprints(agent)
            .iter()
            .any(|fingerprint| detected_fingerprints.contains(fingerprint))
    });
    before.saturating_sub(agents.len())
}

fn is_scan_discovered_agent(agent: &AgentItem) -> bool {
    agent
        .discovery_sources
        .iter()
        .any(|source| is_scan_discovery_source(source))
}

fn is_scan_discovery_source(source: &str) -> bool {
    SCAN_DISCOVERY_SOURCES.iter().any(|item| item == &source)
}

fn refresh_agent_durations(store: &Arc<AppStore>) {
    let offline_timeout = store
        .config
        .lock()
        .map(|config| config.app.offline_timeout_sec)
        .unwrap_or(120);
    let now = now_millis();
    if let Ok(mut snapshot) = store.snapshot.lock() {
        for agent in &mut snapshot.agents {
            if let Some(started_at) = &agent.started_at {
                if let Some(start) = parse_time_millis(started_at) {
                    agent.duration_sec = Some(now.saturating_sub(start) / 1000);
                }
            }
            if let Some(updated) = parse_time_millis(&agent.updated_at) {
                let age = now.saturating_sub(updated) / 1000;
                if age > offline_timeout && matches!(agent.status, AgentStatus::Running) {
                    agent.status = AgentStatus::Offline;
                }
            }
        }
    }
}

fn infer_kind(command: &str) -> Option<AgentKind> {
    let lowered = command.to_lowercase();
    if lowered.contains("claude") {
        Some(AgentKind::ClaudeCode)
    } else if lowered.contains("codex") {
        Some(AgentKind::Codex)
    } else {
        None
    }
}

fn infer_process_kind(command: &str) -> Option<AgentKind> {
    let lowered = command.to_lowercase();
    let ignored_needles = [
        "codex.app/contents",
        "/.codex/plugins/",
        "node_repl",
        "chrome_crashpad_handler",
        "helper.app/contents",
        "sparkle.framework",
        "org.sparkle-project.sparkle",
        "/.vscode/extensions/openai.",
        "agent-pilot",
        "cargo tauri",
        "cc-connect",
        "--permission-prompt-tool stdio",
        "--output-format stream-json",
        "--input-format stream-json",
    ];
    if ignored_needles
        .iter()
        .any(|needle| lowered.contains(needle))
    {
        return None;
    }

    let first = command.split_whitespace().next().unwrap_or_default();
    let binary = first
        .trim_matches('"')
        .trim_matches('\'')
        .rsplit('/')
        .next()
        .unwrap_or(first)
        .to_lowercase();

    match binary.as_str() {
        "claude" | "claude-code" => Some(AgentKind::ClaudeCode),
        "codex" | "codex-cli" => Some(AgentKind::Codex),
        _ => None,
    }
}

fn infer_local_ssh_candidate(
    command: &str,
    pid: u32,
    parent_pid: Option<u32>,
    terminal_pid: Option<u32>,
    tty: Option<String>,
    checked_at: &str,
) -> Option<DiscoveredAgent> {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let first = tokens.first()?;
    let binary = first
        .trim_matches('"')
        .trim_matches('\'')
        .rsplit('/')
        .next()
        .unwrap_or(first)
        .to_lowercase();
    if binary != "ssh" {
        return None;
    }

    let tmux_index = tokens.iter().position(|token| {
        token
            .trim_matches('"')
            .trim_matches('\'')
            .rsplit('/')
            .next()
            .map(|name| name == "tmux")
            .unwrap_or(false)
    });
    let session = tmux_index.and_then(|index| extract_tmux_session(&tokens[index + 1..]));
    let (ssh_user, ssh_host, ssh_port) = extract_ssh_destination(&tokens)?;
    let kind = session
        .as_deref()
        .and_then(infer_kind)
        .or_else(|| infer_kind(command))
        .unwrap_or(AgentKind::Other);
    let port_suffix = ssh_port.map(|port| format!(":{port}")).unwrap_or_default();
    let user_prefix = ssh_user
        .as_ref()
        .map(|user| format!("{user}@"))
        .unwrap_or_default();
    let remote_label = format!("{user_prefix}{ssh_host}{port_suffix}");
    let name = session
        .as_ref()
        .map(|session| format!("{} · {}", kind_label(&kind), session))
        .unwrap_or_else(|| format!("SSH · {remote_label}"));
    let captured_output = session
        .as_deref()
        .and_then(|session| {
            capture_ssh_tmux_output(
                ssh_user.as_ref(),
                &ssh_host,
                ssh_port.unwrap_or(22),
                session,
            )
        })
        .or_else(|| tty.as_deref().and_then(capture_terminal_tab_text_by_tty));
    let (status, last_output) = captured_output
        .as_deref()
        .map(|output| infer_status_from_terminal_text(output, Some(command)))
        .unwrap_or((None, None));

    Some(DiscoveredAgent {
        fingerprint: format!("local-ssh:{pid}"),
        agent_id: None,
        name: Some(name),
        kind,
        location: AgentLocation::Remote,
        machine_label: "Local Mac".to_string(),
        cwd: session
            .as_ref()
            .map(|session| format!("{remote_label} · tmux:{session}"))
            .or_else(|| Some(remote_label)),
        command: Some(command.to_string()),
        last_output,
        status,
        pid: Some(pid),
        parent_pid,
        terminal_pid,
        tty,
        tmux_session: session,
        tmux_pane: None,
        ssh_host: Some(ssh_host),
        ssh_user,
        discovery_source: "local_ssh".to_string(),
        confidence: 0.86,
        detected_at: checked_at.to_string(),
    })
}

fn extract_tmux_session(tokens: &[&str]) -> Option<String> {
    for (index, token) in tokens.iter().enumerate() {
        if (*token == "-t" || *token == "-s") && tokens.get(index + 1).is_some() {
            return tokens
                .get(index + 1)
                .map(|session| clean_shell_token(session));
        }
    }

    tokens
        .iter()
        .skip_while(|token| matches!(**token, "attach" | "attach-session" | "a"))
        .find(|token| !token.starts_with('-'))
        .map(|session| clean_shell_token(session))
}

fn extract_ssh_destination(tokens: &[&str]) -> Option<(Option<String>, String, Option<u16>)> {
    let mut ssh_user = None;
    let mut ssh_host = None;
    let mut ssh_port = None;
    let mut skip_next = false;
    let options_with_values = [
        "-b", "-c", "-e", "-i", "-J", "-l", "-m", "-o", "-p", "-S", "-W",
    ];

    for (index, token) in tokens.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }

        if *token == "-l" {
            ssh_user = tokens.get(index + 1).map(|value| clean_shell_token(value));
            skip_next = true;
            continue;
        }

        if *token == "-p" {
            ssh_port = tokens
                .get(index + 1)
                .and_then(|value| clean_shell_token(value).parse::<u16>().ok());
            skip_next = true;
            continue;
        }

        if let Some(port_text) = token.strip_prefix("-p") {
            ssh_port = port_text.parse::<u16>().ok();
            continue;
        }

        if options_with_values.iter().any(|option| option == token) {
            skip_next = true;
            continue;
        }

        if token.starts_with('-') {
            continue;
        }

        let destination = clean_shell_token(token);
        if ssh_host.is_none() {
            if let Some((user, host)) = destination.split_once('@') {
                ssh_user = Some(user.to_string());
                ssh_host = Some(host.to_string());
            } else {
                ssh_host = Some(destination);
            }
        }
    }

    ssh_host.map(|host| (ssh_user, host, ssh_port))
}

fn clean_shell_token(value: &str) -> String {
    value
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(';')
        .to_string()
}

fn infer_cwd(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .find(|part| part.starts_with('/') || part.starts_with("~/"))
        .map(|part| part.trim_matches('\'').trim_matches('"').to_string())
}

fn dedup(items: Vec<DiscoveredAgent>) -> Vec<DiscoveredAgent> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if seen.insert(item.fingerprint.clone()) {
            out.push(item);
        }
    }
    out
}

fn agent_from_candidate(candidate: DiscoveredAgent, name: String) -> AgentItem {
    let session_name = candidate.tmux_session.clone().unwrap_or_else(|| {
        format!(
            "{}_pid_{}",
            kind_key(&candidate.kind),
            candidate.pid.unwrap_or(0)
        )
    });
    let terminal_target = if candidate.discovery_source == "codex_desktop" {
        build_desktop_app_target(&candidate)
    } else if should_use_process_terminal_target(&candidate) {
        build_process_terminal_target(&candidate)
    } else {
        build_terminal_target(
            &candidate.location,
            "ghostty",
            candidate.cwd.clone(),
            Some(session_name.clone()),
            candidate.ssh_host.clone(),
            candidate.ssh_user.clone(),
            Some(22),
        )
    };
    AgentItem {
        id: candidate
            .agent_id
            .clone()
            .unwrap_or_else(|| format!("agent-{}", now_millis())),
        name: if name.trim().is_empty() {
            candidate.name.clone().unwrap_or_else(|| {
                format!(
                    "{} · {}",
                    kind_label(&candidate.kind),
                    candidate.machine_label
                )
            })
        } else {
            name
        },
        kind: candidate.kind,
        location: candidate.location,
        machine_label: candidate.machine_label,
        project_label: None,
        cwd: candidate.cwd,
        current_task: Some(if candidate.discovery_source == "codex_desktop" {
            "监测 Codex Desktop 审批与任务状态。".to_string()
        } else {
            "刚加入管理，等待下一次状态上报。".to_string()
        }),
        last_output: candidate.last_output.or(candidate.command),
        status: candidate.status.unwrap_or(AgentStatus::Running),
        discovery_state: AgentDiscoveryState::Managed,
        discovery_sources: vec![candidate.discovery_source],
        confidence: Some(candidate.confidence),
        pid: candidate.pid,
        tmux_session: candidate.tmux_session.or_else(|| {
            if terminal_target.target_type == "local-process"
                || terminal_target.target_type == "desktop-app"
            {
                None
            } else {
                Some(session_name)
            }
        }),
        tmux_pane: candidate.tmux_pane,
        ssh_host: candidate.ssh_host,
        ssh_user: candidate.ssh_user,
        started_at: Some(candidate.detected_at),
        updated_at: now_iso(),
        duration_sec: Some(0),
        terminal_target: Some(terminal_target),
    }
}

fn build_terminal_target(
    location: &AgentLocation,
    terminal_app: &str,
    cwd: Option<String>,
    session_name: Option<String>,
    ssh_host: Option<String>,
    ssh_user: Option<String>,
    ssh_port: Option<u16>,
) -> TerminalTarget {
    let session = session_name.unwrap_or_else(|| "agent_pilot".to_string());
    match location {
        AgentLocation::Remote => TerminalTarget {
            target_type: "ssh-tmux".to_string(),
            terminal_app: terminal_app.to_string(),
            process_pid: None,
            terminal_pid: None,
            tty: None,
            process_command: None,
            ssh_host,
            ssh_user,
            ssh_port,
            session_name: Some(session.clone()),
            local_command: None,
            remote_command: Some(format!("tmux attach -t {session} || tmux new -s {session}")),
        },
        AgentLocation::Local => {
            let dir = cwd.unwrap_or_else(|| "~".to_string());
            TerminalTarget {
                target_type: "local-shell".to_string(),
                terminal_app: terminal_app.to_string(),
                process_pid: None,
                terminal_pid: None,
                tty: None,
                process_command: None,
                ssh_host: None,
                ssh_user: None,
                ssh_port: None,
                session_name: Some(session.clone()),
                local_command: Some(format!(
                    "cd {} && tmux attach -t {} || tmux new -s {}",
                    shell_quote(&dir),
                    shell_quote(&session),
                    shell_quote(&session)
                )),
                remote_command: None,
            }
        }
    }
}

fn build_desktop_app_target(candidate: &DiscoveredAgent) -> TerminalTarget {
    TerminalTarget {
        target_type: "desktop-app".to_string(),
        terminal_app: "codex".to_string(),
        process_pid: candidate.pid,
        terminal_pid: candidate.terminal_pid,
        tty: None,
        process_command: candidate.command.clone(),
        ssh_host: None,
        ssh_user: None,
        ssh_port: None,
        session_name: Some("Codex Desktop".to_string()),
        local_command: None,
        remote_command: None,
    }
}

fn terminal_command(target: &TerminalTarget) -> String {
    if target.target_type == "ssh-tmux" {
        let Some(host) = target.ssh_host.clone() else {
            return String::new();
        };
        let user = target
            .ssh_user
            .clone()
            .unwrap_or_else(|| "root".to_string());
        let port = target.ssh_port.unwrap_or(22).to_string();
        let remote = target
            .remote_command
            .clone()
            .or_else(|| {
                target
                    .session_name
                    .clone()
                    .map(|session| format!("tmux attach -t {session} || tmux new -s {session}"))
            })
            .unwrap_or_else(|| "tmux ls".to_string());
        format!("ssh -p {port} {user}@{host} -t {}", shell_quote(&remote))
    } else {
        target
            .local_command
            .clone()
            .or_else(|| {
                target.session_name.clone().map(|session| {
                    format!(
                        "tmux attach -t {} || tmux new -s {}",
                        shell_quote(&session),
                        shell_quote(&session)
                    )
                })
            })
            .unwrap_or_default()
    }
}

fn focus_desktop_app(target: &TerminalTarget) -> bool {
    if let Some(pid) = target.process_pid.or(target.terminal_pid) {
        let script = format!(
            "tell application \"System Events\" to set frontmost of first process whose unix id is {} to true",
            pid
        );
        if Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return true;
        }
    }

    let app_name = match target.terminal_app.as_str() {
        "codex" => "Codex",
        other => other,
    };
    let script = format!("tell application {} to activate", apple_quote(app_name));
    Command::new("osascript")
        .args(["-e", &script])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
        || Command::new("open")
            .args(["-a", app_name])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
}

fn build_process_terminal_target(candidate: &DiscoveredAgent) -> TerminalTarget {
    let pid = candidate.pid.unwrap_or_default();
    let terminal_app = candidate
        .terminal_pid
        .and_then(terminal_app_for_pid)
        .unwrap_or_else(|| "ghostty".to_string());
    let tty_label = candidate
        .tty
        .clone()
        .unwrap_or_else(|| "no-tty".to_string());
    TerminalTarget {
        target_type: "local-process".to_string(),
        terminal_app,
        process_pid: candidate.pid,
        terminal_pid: candidate.terminal_pid,
        tty: candidate.tty.clone(),
        process_command: candidate.command.clone(),
        ssh_host: None,
        ssh_user: None,
        ssh_port: None,
        session_name: Some(format!("{}_pid_{pid}", kind_key(&candidate.kind))),
        local_command: Some(local_process_status_command(
            pid,
            &tty_label,
            candidate.command.as_deref(),
        )),
        remote_command: None,
    }
}

fn local_process_status_command(
    pid: u32,
    tty_label: &str,
    process_command: Option<&str>,
) -> String {
    let mut command = format!(
        "printf '%s\\n' {} {}; ps -p {} -o pid,ppid,tty,command",
        shell_quote("Agent Pilot found this Agent as a running local process."),
        shell_quote(&format!("PID: {pid} · TTY: {tty_label}")),
        pid
    );

    if let Some(process_command) = process_command.filter(|value| !value.trim().is_empty()) {
        command.push_str(&format!(
            "; printf '%s\\n' {} {}",
            shell_quote("Command:"),
            shell_quote(process_command)
        ));
    }

    command.push_str("; printf '\\nPress enter to close...'; read -r _");
    command
}

fn focus_local_process_terminal(target: &TerminalTarget) -> bool {
    if target.terminal_app == "terminal" {
        if let Some(tty) = &target.tty {
            if focus_terminal_tab_by_tty(tty) {
                return true;
            }
        }
    }

    if target.terminal_app == "ghostty" && focus_ghostty_terminal_by_title(target) {
        return true;
    }

    if let Some(terminal_pid) = target.terminal_pid {
        let script = format!(
            "tell application \"System Events\" to set frontmost of first process whose unix id is {} to true",
            terminal_pid
        );
        if Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return true;
        }
    }

    let activate_target = match target.terminal_app.as_str() {
        "terminal" => "Terminal",
        "iterm" => "iTerm",
        _ => "Ghostty",
    };
    let activate_script = format!("tell application \"{}\" to activate", activate_target);
    if Command::new("osascript")
        .args(["-e", &activate_script])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        return true;
    }

    if let Some(command) = &target.local_command {
        open_command_in_terminal(&target.terminal_app, command);
        return true;
    }

    false
}

fn focus_terminal_tab_by_tty(tty: &str) -> bool {
    let short_tty = tty.trim_start_matches("/dev/");
    let full_tty = format!("/dev/{short_tty}");
    let script = format!(
        r#"tell application "Terminal"
  repeat with terminalWindow in windows
    repeat with terminalTab in tabs of terminalWindow
      try
        set tabTty to tty of terminalTab as text
        if tabTty is {full_tty} or tabTty is {short_tty} then
          set selected tab of terminalWindow to terminalTab
          set index of terminalWindow to 1
          activate
          return true
        end if
      end try
    end repeat
  end repeat
end tell
return false"#,
        full_tty = apple_quote(&full_tty),
        short_tty = apple_quote(short_tty)
    );

    Command::new("osascript")
        .args(["-e", &script])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

fn focus_ghostty_terminal_by_title(target: &TerminalTarget) -> bool {
    let Some(process_command) = target
        .process_command
        .as_deref()
        .map(str::trim)
        .filter(|command| command.split_whitespace().count() >= 2)
    else {
        return false;
    };

    let script = format!(
        r#"tell application "Ghostty"
  set matchedTerminal to missing value
  set matchCount to 0
  repeat with ghostTerminal in terminals
    set terminalTitle to name of ghostTerminal as text
    ignoring case
      if terminalTitle contains {match_text} then
        set matchedTerminal to ghostTerminal
        set matchCount to matchCount + 1
      end if
    end ignoring
  end repeat
  if matchCount is 1 then
    focus matchedTerminal
    return true
  end if
end tell
return false"#,
        match_text = apple_quote(process_command)
    );

    Command::new("osascript")
        .args(["-e", &script])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

fn open_local_process_terminal(target: &TerminalTarget) -> bool {
    if target.terminal_pid.is_some() && target.tty.is_some() && focus_local_process_terminal(target)
    {
        return true;
    }

    let command = target
        .process_pid
        .map(|pid| {
            local_process_status_command(
                pid,
                target.tty.as_deref().unwrap_or("no-tty"),
                target.process_command.as_deref(),
            )
        })
        .unwrap_or_else(|| terminal_command(target));
    if !command.is_empty() {
        open_command_in_terminal(&target.terminal_app, &command);
        return true;
    }

    focus_local_process_terminal(target)
}

fn find_terminal_parent_pid(mut pid: Option<u32>) -> Option<u32> {
    for _ in 0..10 {
        let current = pid?;
        let (_, command) = process_parent_and_command(current)?;
        let lowered = command.to_lowercase();
        if lowered.contains("ghostty.app")
            || lowered.contains("terminal.app")
            || lowered.contains("iterm.app")
            || lowered.contains("iterm2.app")
        {
            return Some(current);
        }
        pid = process_parent_and_command(current).map(|(parent, _)| parent);
    }
    None
}

fn terminal_app_for_pid(pid: u32) -> Option<String> {
    let (_, command) = process_parent_and_command(pid)?;
    let lowered = command.to_lowercase();
    if lowered.contains("terminal.app") {
        Some("terminal".to_string())
    } else if lowered.contains("iterm.app") || lowered.contains("iterm2.app") {
        Some("iterm".to_string())
    } else if lowered.contains("ghostty.app") {
        Some("ghostty".to_string())
    } else {
        None
    }
}

fn should_use_process_terminal_target(candidate: &DiscoveredAgent) -> bool {
    candidate.discovery_source == "local_ssh"
        || candidate.discovery_source == "local_ssh_tmux"
        || (candidate.discovery_source == "local_process"
            && matches!(candidate.location, AgentLocation::Local)
            && candidate.tmux_session.is_none())
}

fn process_parent_and_command(pid: u32) -> Option<(u32, String)> {
    let output = Command::new("ps")
        .args(["-o", "ppid=,command=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    let (parent_text, command) = line.split_once(' ')?;
    let parent = parent_text.trim().parse::<u32>().ok()?;
    Some((parent, command.trim().to_string()))
}

fn agent_fingerprints(agent: &AgentItem) -> Vec<String> {
    let mut fingerprints = Vec::new();
    if let (Some(pid), kind) = (agent.pid, &agent.kind) {
        if agent
            .discovery_sources
            .iter()
            .any(|source| source == "local_ssh" || source == "local_ssh_tmux")
        {
            fingerprints.push(format!("local-ssh:{pid}"));
            if let Some(session) = &agent.tmux_session {
                fingerprints.push(format!("local-ssh-tmux:{pid}:{session}"));
            }
        }
        match &agent.location {
            AgentLocation::Local => fingerprints.push(format!("local:{}:{pid}", kind_key(kind))),
            AgentLocation::Remote => {
                if let Some(command) = &agent.last_output {
                    fingerprints.push(format!(
                        "remote:{}@{}:{}:{}:{}",
                        agent.ssh_user.clone().unwrap_or_default(),
                        agent.ssh_host.clone().unwrap_or_default(),
                        kind_key(kind),
                        pid,
                        command
                    ));
                }
            }
        }
    }
    if let (Some(session), Some(cwd)) = (&agent.tmux_session, &agent.cwd) {
        let pane = agent.tmux_pane.clone().unwrap_or_else(|| "0".to_string());
        match &agent.location {
            AgentLocation::Local => fingerprints.push(format!(
                "local-tmux:{}:{}:{}:{}",
                kind_key(&agent.kind),
                session,
                pane,
                cwd
            )),
            AgentLocation::Remote => fingerprints.push(format!(
                "remote-tmux:{}@{}:{}:{}:{}:{}",
                agent.ssh_user.clone().unwrap_or_default(),
                agent.ssh_host.clone().unwrap_or_default(),
                kind_key(&agent.kind),
                session,
                pane,
                cwd
            )),
        }
    }
    fingerprints.push(format!("manual:{}", agent.id));
    fingerprints
}

fn load_or_create_config() -> DeskConfig {
    let path = config_path();
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(mut config) = serde_json::from_str::<DeskConfig>(&text) {
            config = normalize_config(config);
            config.agents = sanitize_agents(config.agents);
            save_config(&config);
            return config;
        }
    }
    let config = normalize_config(default_config());
    save_config(&config);
    config
}

fn normalize_config(mut config: DeskConfig) -> DeskConfig {
    config.discovery.auto_add_mode = "auto".to_string();
    if config.discovery.local_scan_interval_sec <= 45 {
        config.discovery.local_scan_interval_sec = 60;
    }
    if config.discovery.remote_scan_interval_sec <= 45 {
        config.discovery.remote_scan_interval_sec = 60;
    }
    config
}

fn default_config() -> DeskConfig {
    DeskConfig {
        app: AppConfig {
            listen_host: "127.0.0.1".to_string(),
            listen_port: 8787,
            offline_timeout_sec: 120,
            default_terminal: "ghostty".to_string(),
        },
        discovery: DiscoveryConfig {
            enabled: true,
            scan_on_startup: true,
            local_scan_interval_sec: 60,
            remote_scan_interval_sec: 60,
            auto_add_mode: "auto".to_string(),
            scan_local_processes: true,
            scan_local_tmux: true,
            scan_remote_hosts: true,
            trusted_commands: vec!["claude".to_string(), "codex".to_string()],
            ignored_fingerprints: Vec::new(),
        },
        remote_hosts: Vec::new(),
        agents: Vec::new(),
    }
}

fn save_config(config: &DeskConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}

fn persist_agents(store: &Arc<AppStore>, agents: Vec<AgentItem>) {
    if let Ok(mut config) = store.config.lock() {
        config.agents = sanitize_agents(agents);
        save_config(&config);
    }
}

fn sanitize_agents(agents: Vec<AgentItem>) -> Vec<AgentItem> {
    agents
        .into_iter()
        .filter(|agent| {
            if agent.id.starts_with("demo-") {
                return false;
            }

            if !agent
                .discovery_sources
                .iter()
                .any(|source| source == "local_process")
            {
                return true;
            }

            let text = format!(
                "{} {}",
                agent.cwd.clone().unwrap_or_default(),
                agent.last_output.clone().unwrap_or_default()
            )
            .to_lowercase();
            !is_ignored_process_text(&text)
        })
        .collect()
}

fn is_ignored_process_text(lowered: &str) -> bool {
    let ignored_needles = [
        "codex.app/contents",
        "/.codex/plugins/",
        "node_repl",
        "chrome_crashpad_handler",
        "helper.app/contents",
        "sparkle.framework",
        "org.sparkle-project.sparkle",
        "/.vscode/extensions/openai.",
        "agent-pilot",
        "cargo tauri",
        "cc-connect",
        "--permission-prompt-tool stdio",
        "--output-format stream-json",
        "--input-format stream-json",
    ];
    ignored_needles
        .iter()
        .any(|needle| lowered.contains(needle))
}

fn push_scan_report(store: &Arc<AppStore>, report: ScanReport) {
    if let Ok(mut snapshot) = store.snapshot.lock() {
        snapshot.scan_reports.insert(0, report);
        snapshot.scan_reports.truncate(12);
        snapshot.last_updated = now_iso();
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".agent-pilot").join("config.json")
}

fn home_path(relative: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(relative))
}

fn parse_body(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|_| json!({}))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(header: &str) -> usize {
    header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn write_json_response(stream: &mut TcpStream, status: u16, payload: Value) {
    let status_text = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    let body = if status == 204 {
        String::new()
    } else {
        serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn kind_key(kind: &AgentKind) -> &'static str {
    match kind {
        AgentKind::Codex => "codex",
        AgentKind::CodexDesktop => "codex_desktop",
        AgentKind::ClaudeCode => "claude_code",
        AgentKind::Other => "other",
        AgentKind::Unknown => "unknown",
    }
}

fn kind_label(kind: &AgentKind) -> &'static str {
    match kind {
        AgentKind::Codex => "Codex CLI",
        AgentKind::CodexDesktop => "Codex Desktop",
        AgentKind::ClaudeCode => "Claude Code",
        AgentKind::Other => "Agent",
        AgentKind::Unknown => "Agent",
    }
}

fn now_iso() -> String {
    if let Ok(output) = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    format!("{}Z", now_millis())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn parse_time_millis(value: &str) -> Option<u64> {
    if let Ok(number) = value.trim_end_matches('Z').parse::<u64>() {
        return Some(number);
    }
    let output = Command::new("date")
        .args(["-j", "-u", "-f", "%Y-%m-%dT%H:%M:%SZ", value, "+%s"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let seconds = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(seconds * 1000)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn apple_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
