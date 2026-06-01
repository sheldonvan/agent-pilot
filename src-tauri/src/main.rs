#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::{HashMap, HashSet},
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Output, Stdio},
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
    ssh_password: Option<String>,
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
    ssh_port: Option<u16>,
    #[serde(default)]
    ssh_password_required: bool,
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
    vscode_uri: Option<String>,
    ghostty_terminal_id: Option<String>,
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
    ssh_port: Option<u16>,
    #[serde(default)]
    ssh_password_required: bool,
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
    ssh_port: Option<u16>,
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
    status_observations: Mutex<HashMap<String, StatusObservation>>,
}

#[derive(Debug, Clone)]
struct StatusObservation {
    output_hash: u64,
    output_changed_at: u64,
    last_seen_at: u64,
}

#[derive(Debug, Clone)]
struct SshEndpoint {
    host: String,
    user: Option<String>,
    port: Option<u16>,
    password: Option<String>,
}

#[derive(Debug, Clone)]
struct GhosttyTerminal {
    id: String,
    name: String,
    working_directory: String,
}

#[derive(Debug, Clone)]
struct VscodeRemoteSession {
    local_server_pid: u32,
    code_pid: Option<u32>,
    endpoint: SshEndpoint,
    data_file_path: Option<PathBuf>,
    socks_port: Option<u16>,
    remote_port: Option<u16>,
    exec_server_token: Option<String>,
    local_forward_port: Option<u16>,
}

#[derive(Debug, Clone)]
struct AgentRuntimeHint {
    status: Option<AgentStatus>,
    last_output: Option<String>,
    cwd: Option<String>,
}

const SCAN_DISCOVERY_SOURCES: [&str; 10] = [
    "local_process",
    "local_ssh",
    "local_ssh_tmux",
    "local_tmux",
    "codex_desktop",
    "remote_process",
    "remote_tmux",
    "vscode_remote_ssh",
    "vscode_remote_process",
    "vscode_remote_tmux",
];

const APP_VERSION: &str = "0.2.0";
const IDLE_PROMPT_STABLE_MS: u64 = 20_000;
const VSCODE_REMOTE_EXEC_HELPER: &str = include_str!("../helpers/vscode_remote_exec.cjs");

fn main() {
    let config = load_or_create_config();
    let collector_url = format!(
        "http://{}:{}",
        config.app.listen_host, config.app.listen_port
    );
    let snapshot = DeskSnapshot {
        version: APP_VERSION.to_string(),
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
        status_observations: Mutex::new(HashMap::new()),
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
            save_ssh_password,
            post_event,
            open_terminal,
            open_config_file,
            open_permission_settings
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
fn save_ssh_password(
    ssh_host: String,
    ssh_user: Option<String>,
    ssh_port: Option<u16>,
    password: String,
    state: State<Arc<AppStore>>,
) -> ApiOk {
    save_ssh_password_inner(&state, &ssh_host, ssh_user.as_deref(), ssh_port, &password)
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

#[tauri::command]
fn open_permission_settings(pane: String) -> ApiOk {
    open_permission_settings_inner(&pane)
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
                "version": APP_VERSION,
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
        ("POST", "/api/ssh/password") => {
            let payload: Value = parse_body(body);
            let ssh_host = payload
                .get("sshHost")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let ssh_user = payload.get("sshUser").and_then(Value::as_str);
            let ssh_port = payload
                .get("sshPort")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok());
            let password = payload
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (
                200,
                json!(save_ssh_password_inner(
                    store, ssh_host, ssh_user, ssh_port, password
                )),
            )
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
        ("POST", "/api/permissions/open") => {
            let payload: Value = parse_body(body);
            let pane = payload
                .get("pane")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (200, json!(open_permission_settings_inner(pane)))
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
        let (items, report) = scan_local_processes(&config.remote_hosts);
        detected.extend(items);
        reports.push(report);
        let (items, report) = scan_codex_desktop();
        detected.extend(items);
        reports.push(report);
        let (items, report) = scan_vscode_remote_ssh(&config.remote_hosts);
        detected.extend(items);
        reports.push(report);
    }
    if config.discovery.enabled && config.discovery.scan_local_tmux {
        let (items, report) = scan_local_tmux();
        detected.extend(items);
        reports.push(report);
    }
    let active_session_endpoints = active_ssh_session_endpoints(&detected);
    if config.discovery.enabled && config.discovery.scan_remote_hosts {
        for host in config.remote_hosts.iter().filter(|host| host.scan_enabled) {
            if active_session_endpoints.iter().any(|endpoint| {
                remote_host_matches_endpoint(
                    host,
                    endpoint.user.as_deref(),
                    &endpoint.host,
                    endpoint.port,
                )
            }) {
                reports.push(ScanReport {
                    source: "remote_hosts".to_string(),
                    label: host.label.clone(),
                    ok: true,
                    message:
                        "skipped global remote scan because an active SSH terminal session is monitored directly"
                            .to_string(),
                    checked_at: now_iso(),
                });
                continue;
            }
            let (process_items, process_report) = scan_remote_processes(host);
            let (tmux_items, tmux_report) = scan_remote_tmux(host);
            detected.extend(process_items);
            detected.extend(tmux_items);
            reports.push(process_report);
            reports.push(tmux_report);
        }
    }

    let mut detected = dedup(detected);
    apply_status_observations(store, &mut detected);
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

fn active_ssh_session_endpoints(items: &[DiscoveredAgent]) -> Vec<SshEndpoint> {
    let mut seen = HashSet::new();
    let mut endpoints = Vec::new();
    for item in items.iter().filter(|item| {
        item.discovery_source == "local_ssh" || item.discovery_source == "vscode_remote_ssh"
    }) {
        let Some(host) = item.ssh_host.clone() else {
            continue;
        };
        let endpoint = SshEndpoint {
            host,
            user: item.ssh_user.clone(),
            port: item.ssh_port,
            password: None,
        };
        if seen.insert(endpoint_label(&endpoint)) {
            endpoints.push(endpoint);
        }
    }
    endpoints
}

fn apply_status_observations(store: &Arc<AppStore>, items: &mut [DiscoveredAgent]) {
    let previous_statuses = store
        .snapshot
        .lock()
        .map(|snapshot| {
            let mut statuses = HashMap::new();
            for agent in &snapshot.agents {
                for fingerprint in agent_fingerprints(agent) {
                    statuses.insert(fingerprint, agent.status.clone());
                }
            }
            statuses
        })
        .unwrap_or_default();

    let now = now_millis();
    let mut current_output_keys = HashSet::new();
    let Ok(mut observations) = store.status_observations.lock() else {
        return;
    };

    for item in items {
        let Some(output) = item.last_output.as_deref() else {
            continue;
        };
        current_output_keys.insert(item.fingerprint.clone());

        if matches!(
            item.status,
            Some(AgentStatus::WaitingAttention | AgentStatus::Error | AgentStatus::Offline)
        ) {
            continue;
        }

        let output_hash = stable_hash(output);
        let observation =
            observations
                .entry(item.fingerprint.clone())
                .or_insert(StatusObservation {
                    output_hash,
                    output_changed_at: now,
                    last_seen_at: now,
                });

        let output_changed = observation.output_hash != output_hash;
        if output_changed {
            observation.output_hash = output_hash;
            observation.output_changed_at = now;
        }
        observation.last_seen_at = now;

        if matches!(item.status, Some(AgentStatus::Done)) && output_changed {
            item.status = Some(AgentStatus::Running);
            continue;
        }

        if item.status.is_some() {
            continue;
        }

        if output_changed {
            item.status = Some(AgentStatus::Running);
            continue;
        }

        let stable_for = now.saturating_sub(observation.output_changed_at);
        if contains_waiting_input_prompt(output) && stable_for >= IDLE_PROMPT_STABLE_MS {
            item.status = Some(AgentStatus::Done);
        } else if !matches!(
            previous_statuses.get(&item.fingerprint),
            Some(AgentStatus::WaitingAttention | AgentStatus::Error | AgentStatus::Offline)
        ) {
            item.status = Some(AgentStatus::Running);
        }
    }

    observations.retain(|fingerprint, _| current_output_keys.contains(fingerprint));
}

fn stable_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
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

fn save_ssh_password_inner(
    store: &Arc<AppStore>,
    ssh_host: &str,
    ssh_user: Option<&str>,
    ssh_port: Option<u16>,
    password: &str,
) -> ApiOk {
    let ssh_host = ssh_host.trim();
    let ssh_user = ssh_user.and_then(non_empty_string);
    let password = password.trim();
    if ssh_host.is_empty() || password.is_empty() {
        return ApiOk { ok: false };
    }
    let ssh_port = ssh_port.unwrap_or(22);

    if let Ok(mut config) = store.config.lock() {
        if let Some(host) = config.remote_hosts.iter_mut().find(|host| {
            remote_host_matches_endpoint(host, ssh_user.as_deref(), ssh_host, Some(ssh_port))
        }) {
            if let Some(user) = ssh_user.as_ref() {
                if host.ssh_user.trim().is_empty() {
                    host.ssh_user = user.clone();
                }
            }
            host.ssh_port = ssh_port;
            host.ssh_password = Some(password.to_string());
            host.scan_enabled = false;
        } else {
            let endpoint = SshEndpoint {
                host: ssh_host.to_string(),
                user: ssh_user.clone(),
                port: Some(ssh_port),
                password: None,
            };
            config.remote_hosts.push(RemoteHost {
                id: format!("ssh-{}", config_id_fragment(&endpoint_label(&endpoint))),
                label: endpoint_label(&endpoint),
                ssh_host: ssh_host.to_string(),
                ssh_user: ssh_user.clone().unwrap_or_default(),
                ssh_port,
                ssh_password: Some(password.to_string()),
                scan_enabled: false,
            });
        }
        save_config(&config);
    } else {
        return ApiOk { ok: false };
    }

    if let Ok(mut snapshot) = store.snapshot.lock() {
        for agent in &mut snapshot.agents {
            if endpoint_matches_agent(agent, ssh_user.as_deref(), ssh_host, Some(ssh_port)) {
                agent.ssh_password_required = false;
                agent.last_output =
                    Some("SSH 密码已保存到本机配置；下次扫描会尝试用它读取远程状态。".to_string());
                agent.updated_at = now_iso();
            }
        }
        for candidate in &mut snapshot.candidates {
            if endpoint_matches_candidate(candidate, ssh_user.as_deref(), ssh_host, Some(ssh_port))
            {
                candidate.ssh_password_required = false;
            }
        }
        snapshot.last_updated = now_iso();
        persist_agents(store, snapshot.agents.clone());
    }

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
        ssh_port,
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
            ssh_port,
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
        ssh_port,
        started_at: Some(now.clone()),
        updated_at: now.clone(),
        duration_sec: Some(0),
        terminal_target,
        ssh_password_required: false,
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
                ssh_port: agent.ssh_port,
                ssh_password_required: false,
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

fn open_permission_settings_inner(pane: &str) -> ApiOk {
    let url = match pane {
        "automation" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
        }
        "full_disk" => "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
        _ => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    };
    ApiOk {
        ok: Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| true)
            .unwrap_or(false),
    }
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
                .args(["-a", "Ghostty", "--args", "-e", "zsh", "-lc", &command])
                .spawn();
        }
    }
}

fn scan_local_processes(remote_hosts: &[RemoteHost]) -> (Vec<DiscoveredAgent>, ScanReport) {
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
            if is_agent_pilot_internal_ssh_command(&command) {
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
                remote_hosts,
                &checked_at,
            ) {
                return Some(candidate);
            }
            let kind = infer_process_kind(&command)?;
            let mut cwd = infer_cwd(&command);
            let confidence = if cwd.is_some() { 0.82 } else { 0.72 };
            let fingerprint = format!("local:{}:{}", kind_key(&kind), pid);
            let (mut status, mut last_output) = tty
                .as_deref()
                .and_then(capture_terminal_tab_text_by_tty)
                .map(|output| infer_status_from_terminal_text(&output, Some(&command)))
                .unwrap_or((None, None));
            if let Some(runtime) = local_agent_runtime_hint(&kind, pid) {
                if runtime.status.is_some() {
                    status = runtime.status;
                }
                if runtime.last_output.is_some() && last_output.is_none() {
                    last_output = runtime.last_output;
                }
                if runtime.cwd.is_some() {
                    cwd = runtime.cwd;
                }
            }
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
                ssh_port: None,
                ssh_password_required: false,
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
        None,
        "local_tmux",
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
            ssh_port: None,
            ssh_password_required: false,
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

fn local_agent_runtime_hint(kind: &AgentKind, pid: u32) -> Option<AgentRuntimeHint> {
    match kind {
        AgentKind::ClaudeCode => {
            let path = home_path(&format!(".claude/sessions/{pid}.json"))?;
            let text = fs::read_to_string(path).ok()?;
            parse_agent_runtime_hint(kind, Some(pid), &text)
        }
        _ => None,
    }
}

fn parse_agent_runtime_hint(
    kind: &AgentKind,
    _pid: Option<u32>,
    text: &str,
) -> Option<AgentRuntimeHint> {
    match kind {
        AgentKind::ClaudeCode => parse_claude_runtime_hint(text),
        _ => None,
    }
}

fn parse_claude_runtime_hint(text: &str) -> Option<AgentRuntimeHint> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let raw_status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let waiting_for = value
        .get("waitingFor")
        .and_then(Value::as_str)
        .and_then(non_empty_string);
    let cwd = value
        .get("cwd")
        .and_then(Value::as_str)
        .and_then(non_empty_string);

    let status = match raw_status.as_str() {
        "waiting" | "blocked" | "approval" | "needs_approval" => {
            Some(AgentStatus::WaitingAttention)
        }
        "running" | "busy" | "working" => Some(AgentStatus::Running),
        "done" | "complete" | "completed" | "idle" | "stopped" => Some(AgentStatus::Done),
        "error" | "failed" => Some(AgentStatus::Error),
        _ if waiting_for.is_some() => Some(AgentStatus::WaitingAttention),
        _ => None,
    };
    let last_output = match status.as_ref() {
        Some(AgentStatus::WaitingAttention) => Some(
            waiting_for
                .map(|reason| format!("Claude Code 等待处理：{reason}"))
                .unwrap_or_else(|| "Claude Code 等待处理。".to_string()),
        ),
        Some(AgentStatus::Running) => Some("Claude Code 正在运行。".to_string()),
        Some(AgentStatus::Done) => Some("Claude Code 已完成。".to_string()),
        Some(AgentStatus::Error) => Some("Claude Code 报告执行错误。".to_string()),
        _ => None,
    };

    Some(AgentRuntimeHint {
        status,
        last_output,
        cwd,
    })
}

fn endpoint_from_remote_host(host: &RemoteHost) -> SshEndpoint {
    SshEndpoint {
        host: host.ssh_host.clone(),
        user: non_empty_string(&host.ssh_user),
        port: Some(host.ssh_port),
        password: host
            .ssh_password
            .clone()
            .and_then(|value| non_empty_string(&value)),
    }
}

fn scan_remote_process_candidates(
    endpoint: &SshEndpoint,
    machine_label: &str,
    discovery_source: &str,
    confidence: f32,
    checked_at: &str,
) -> Result<Vec<DiscoveredAgent>, String> {
    let output =
        ssh_output(endpoint, remote_agent_process_script()).map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(parse_remote_process_lines(
        &String::from_utf8_lossy(&output.stdout),
        endpoint,
        machine_label,
        discovery_source,
        confidence,
        checked_at,
    ))
}

fn remote_agent_process_script() -> &'static str {
    "for pid in $(pgrep -f '([c]laude|[c]odex)' 2>/dev/null || true); do cmd=$(ps -p \"$pid\" -o args= 2>/dev/null || true); [ -n \"$cmd\" ] || continue; cwd=$(readlink \"/proc/$pid/cwd\" 2>/dev/null || true); runtime=\"\"; if [ -r \"$HOME/.claude/sessions/$pid.json\" ]; then runtime=$(tr '\\n\\t' '  ' < \"$HOME/.claude/sessions/$pid.json\"); fi; printf '%s\\t%s\\t%s\\t%s\\n' \"$pid\" \"$cwd\" \"$cmd\" \"$runtime\"; done"
}

fn parse_remote_process_lines(
    text: &str,
    endpoint: &SshEndpoint,
    machine_label: &str,
    discovery_source: &str,
    confidence: f32,
    checked_at: &str,
) -> Vec<DiscoveredAgent> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let pid_text = parts.next()?.trim();
            let cwd_text = parts.next().unwrap_or_default().trim();
            let command = parts.next()?.trim();
            let runtime_text = parts.next().unwrap_or_default().trim();
            let pid = pid_text.parse::<u32>().ok()?;
            let kind = infer_process_kind(command)?;
            let mut cwd = if cwd_text.is_empty() {
                infer_cwd(command)
            } else {
                Some(cwd_text.to_string())
            };
            let runtime = parse_agent_runtime_hint(&kind, Some(pid), runtime_text);
            if let Some(runtime_cwd) = runtime.as_ref().and_then(|runtime| runtime.cwd.clone()) {
                cwd = Some(runtime_cwd);
            }
            let fingerprint = format!(
                "remote:{}@{}:{}:{}",
                endpoint.user.clone().unwrap_or_default(),
                endpoint.host,
                kind_key(&kind),
                pid
            );
            Some(DiscoveredAgent {
                fingerprint,
                agent_id: None,
                name: None,
                kind,
                location: AgentLocation::Remote,
                machine_label: machine_label.to_string(),
                cwd,
                command: Some(command.to_string()),
                last_output: runtime
                    .as_ref()
                    .and_then(|runtime| runtime.last_output.clone()),
                status: runtime.and_then(|runtime| runtime.status),
                pid: Some(pid),
                parent_pid: None,
                terminal_pid: None,
                tty: None,
                tmux_session: None,
                tmux_pane: None,
                ssh_host: Some(endpoint.host.clone()),
                ssh_user: endpoint.user.clone(),
                ssh_port: endpoint.port,
                ssh_password_required: false,
                discovery_source: discovery_source.to_string(),
                confidence,
                detected_at: checked_at.to_string(),
            })
        })
        .collect()
}

fn ssh_output(endpoint: &SshEndpoint, remote: &str) -> std::io::Result<std::process::Output> {
    if endpoint
        .password
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        if let Some(output) = ssh_output_with_saved_password(endpoint, remote) {
            return output;
        }
    }

    let mut command = Command::new("ssh");
    command
        .arg("-o")
        .arg("ConnectTimeout=2")
        .arg("-o")
        .arg("BatchMode=yes");
    if let Some(port) = endpoint.port {
        command.arg("-p").arg(port.to_string());
    }
    command.arg(ssh_target(endpoint.user.as_deref(), &endpoint.host));
    command.arg(remote);
    command.output()
}

fn ssh_output_with_saved_password(
    endpoint: &SshEndpoint,
    remote: &str,
) -> Option<std::io::Result<std::process::Output>> {
    let password = endpoint.password.as_deref()?.trim();
    if password.is_empty() {
        return None;
    }

    let pass_path = std::env::temp_dir().join(format!(
        "agent-pilot-sshpass-{}-{}",
        std::process::id(),
        now_millis()
    ));
    if fs::write(&pass_path, password).is_err() {
        return None;
    }
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(&pass_path, fs::Permissions::from_mode(0o600));
    }

    let output = if let Some(sshpass) = command_path("sshpass") {
        let mut command = Command::new(sshpass);
        command
            .arg("-f")
            .arg(&pass_path)
            .arg("ssh")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg("-o")
            .arg("BatchMode=no")
            .arg("-o")
            .arg("NumberOfPasswordPrompts=1")
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new");
        if let Some(port) = endpoint.port {
            command.arg("-p").arg(port.to_string());
        }
        command.arg(ssh_target(endpoint.user.as_deref(), &endpoint.host));
        command.arg(remote);
        command.output()
    } else {
        ssh_output_with_askpass(endpoint, remote, &pass_path)
    };

    let _ = fs::remove_file(pass_path);
    Some(output)
}

fn ssh_output_with_askpass(
    endpoint: &SshEndpoint,
    remote: &str,
    pass_path: &PathBuf,
) -> std::io::Result<std::process::Output> {
    let askpass_path = std::env::temp_dir().join(format!(
        "agent-pilot-askpass-{}-{}",
        std::process::id(),
        now_millis()
    ));
    let script = format!(
        "#!/bin/sh\ncat {}\n",
        shell_quote(&pass_path.to_string_lossy())
    );
    fs::write(&askpass_path, script)?;
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(&askpass_path, fs::Permissions::from_mode(0o700));
    }

    let mut command = Command::new("ssh");
    command
        .env("SSH_ASKPASS", &askpass_path)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("DISPLAY", ":0")
        .stdin(Stdio::null())
        .arg("-o")
        .arg("ConnectTimeout=5")
        .arg("-o")
        .arg("BatchMode=no")
        .arg("-o")
        .arg("NumberOfPasswordPrompts=1")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new");
    if let Some(port) = endpoint.port {
        command.arg("-p").arg(port.to_string());
    }
    command.arg(ssh_target(endpoint.user.as_deref(), &endpoint.host));
    command.arg(remote);
    let output = command.output();
    let _ = fs::remove_file(askpass_path);
    output
}

fn ssh_command_needs_password(endpoint: &SshEndpoint) -> bool {
    let mut command = Command::new("ssh");
    command
        .arg("-o")
        .arg("ConnectTimeout=2")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("NumberOfPasswordPrompts=0")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new");
    if let Some(port) = endpoint.port {
        command.arg("-p").arg(port.to_string());
    }
    command.arg(ssh_target(endpoint.user.as_deref(), &endpoint.host));
    command.arg("true");

    let Ok(output) = command.output() else {
        return false;
    };
    if output.status.success() {
        return false;
    }
    ssh_error_mentions_password(&String::from_utf8_lossy(&output.stderr))
}

fn ssh_error_mentions_password(stderr: &str) -> bool {
    let lowered = stderr.to_lowercase();
    [
        "permission denied",
        "password",
        "publickey",
        "keyboard-interactive",
        "authentication",
        "authentications that can continue",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn command_path(command: &str) -> Option<String> {
    for prefix in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        let path = PathBuf::from(prefix).join(command);
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }
    let output = Command::new("/usr/bin/which").arg(command).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn ssh_target(user: Option<&str>, host: &str) -> String {
    match user.filter(|value| !value.trim().is_empty()) {
        Some(user) => format!("{user}@{host}"),
        None => host.to_string(),
    }
}

fn endpoint_label(endpoint: &SshEndpoint) -> String {
    let user_prefix = endpoint
        .user
        .as_ref()
        .map(|user| format!("{user}@"))
        .unwrap_or_default();
    let port_suffix = endpoint
        .port
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    format!("{user_prefix}{}{port_suffix}", endpoint.host)
}

fn vscode_remote_fingerprint(endpoint: &SshEndpoint) -> String {
    format!(
        "vscode-remote-ssh:{}@{}:{:?}",
        endpoint.user.clone().unwrap_or_default(),
        endpoint.host,
        endpoint.port
    )
}

fn vscode_remote_authority(endpoint: &SshEndpoint) -> String {
    match endpoint.user.as_deref().filter(|value| !value.is_empty()) {
        Some(user) => format!("{user}@{}", endpoint.host),
        None => endpoint.host.clone(),
    }
}

fn vscode_remote_uri(endpoint: &SshEndpoint, cwd: Option<&str>) -> String {
    let authority = uri_encode_component(&vscode_remote_authority(endpoint));
    let path = cwd
        .map(str::trim)
        .filter(|value| value.starts_with('/'))
        .unwrap_or("/");
    format!(
        "vscode://vscode-remote/ssh-remote+{}{}",
        authority,
        uri_encode_path(path)
    )
}

fn vscode_remote_uri_from_candidate(candidate: &DiscoveredAgent) -> Option<String> {
    let host = candidate.ssh_host.clone()?;
    let endpoint = SshEndpoint {
        host,
        user: candidate.ssh_user.clone(),
        port: candidate.ssh_port,
        password: None,
    };
    Some(vscode_remote_uri(&endpoint, candidate.cwd.as_deref()))
}

fn vscode_file_uri(cwd: Option<&str>) -> Option<String> {
    let path = cwd.map(str::trim).filter(|value| value.starts_with('/'))?;
    Some(format!("vscode://file{}", uri_encode_path(path)))
}

fn uri_encode_component(value: &str) -> String {
    uri_encode_with_slash_mode(value, false)
}

fn uri_encode_path(value: &str) -> String {
    uri_encode_with_slash_mode(value, true)
}

fn uri_encode_with_slash_mode(value: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for &byte in value.as_bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (keep_slash && byte == b'/');
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn scan_remote_processes(host: &RemoteHost) -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let endpoint = endpoint_from_remote_host(host);
    let result =
        scan_remote_process_candidates(&endpoint, &host.label, "remote_process", 0.68, &checked_at);
    let Ok(items) = result else {
        return (
            Vec::new(),
            ScanReport {
                source: "remote_process".to_string(),
                label: host.label.clone(),
                ok: false,
                message: "ssh command failed or authentication is not available".to_string(),
                checked_at,
            },
        );
    };
    let count = items.len();
    (
        items,
        ScanReport {
            source: "remote_process".to_string(),
            label: host.label.clone(),
            ok: true,
            message: format!("found {count} remote process candidates"),
            checked_at,
        },
    )
}

fn scan_remote_tmux(host: &RemoteHost) -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let endpoint = endpoint_from_remote_host(host);
    let result = scan_remote_tmux_candidates(&endpoint, &host.label, "remote_tmux", &checked_at);
    let Ok(mut items) = result else {
        return (
            Vec::new(),
            ScanReport {
                source: "remote_tmux".to_string(),
                label: host.label.clone(),
                ok: false,
                message: "ssh command failed or authentication is not available".to_string(),
                checked_at,
            },
        );
    };
    enrich_remote_tmux_status(&mut items, host);
    let count = items.len();
    (
        items,
        ScanReport {
            source: "remote_tmux".to_string(),
            label: host.label.clone(),
            ok: true,
            message: format!("found {count} remote tmux candidates"),
            checked_at,
        },
    )
}

fn scan_vscode_remote_ssh(remote_hosts: &[RemoteHost]) -> (Vec<DiscoveredAgent>, ScanReport) {
    let checked_at = now_iso();
    let mut sessions = discover_vscode_remote_sessions();
    if sessions.is_empty() {
        return (
            Vec::new(),
            ScanReport {
                source: "vscode_remote_ssh".to_string(),
                label: "VS Code Remote-SSH".to_string(),
                ok: true,
                message: "no active VS Code Remote-SSH sessions".to_string(),
                checked_at,
            },
        );
    }

    let mut items = Vec::new();
    let mut monitored = 0;
    for session in &mut sessions {
        session.endpoint.password = matching_remote_host_password(
            remote_hosts,
            session.endpoint.user.as_deref(),
            &session.endpoint.host,
            session.endpoint.port,
        );
        let label = format!("VS Code SSH · {}", endpoint_label(&session.endpoint));
        let mut session_item = vscode_remote_session_candidate(session, &label, &checked_at);
        if let Ok(child_items) =
            scan_vscode_terminal_process_candidates(session, &label, &checked_at)
        {
            monitored += 1;
            apply_vscode_remote_child_status(&mut session_item, &child_items);
        } else if session.endpoint.password.is_none()
            && ssh_command_needs_password(&session.endpoint)
        {
            session_item.ssh_password_required = true;
            session_item.status = Some(AgentStatus::WaitingAttention);
            session_item.last_output = Some(
                "这个 VS Code Remote-SSH 会话需要 SSH 密码，保存后 Agent Pilot 才能在后台补充远端状态扫描。"
                    .to_string(),
            );
        }
        items.push(session_item);
    }

    let count = items.len();
    let message = if monitored > 0 {
        format!(
            "found {count} VS Code SSH session(s); monitored integrated terminals for {monitored} session(s)"
        )
    } else {
        format!(
            "found {count} VS Code SSH session(s); waiting for an accessible VS Code terminal channel"
        )
    };

    (
        items,
        ScanReport {
            source: "vscode_remote_ssh".to_string(),
            label: "VS Code Remote-SSH".to_string(),
            ok: monitored > 0,
            message,
            checked_at,
        },
    )
}

fn apply_vscode_remote_child_status(
    session_item: &mut DiscoveredAgent,
    child_items: &[DiscoveredAgent],
) {
    if let Some(attention_item) = child_items
        .iter()
        .find(|item| matches!(item.status, Some(AgentStatus::WaitingAttention)))
    {
        session_item.kind = attention_item.kind.clone();
        session_item.command = attention_item
            .command
            .clone()
            .or(session_item.command.clone());
        session_item.confidence = session_item.confidence.max(attention_item.confidence);
        session_item.status = Some(AgentStatus::WaitingAttention);
        session_item.last_output = attention_item.last_output.clone().or_else(|| {
            Some(format!(
                "VS Code Remote-SSH 中的 {} 等待处理。",
                kind_label(&attention_item.kind)
            ))
        });
        session_item.cwd = attention_item.cwd.clone().or(session_item.cwd.clone());
        return;
    }

    if let Some(running_item) = child_items
        .iter()
        .find(|item| matches!(item.status, Some(AgentStatus::Running)))
    {
        session_item.kind = running_item.kind.clone();
        session_item.command = running_item
            .command
            .clone()
            .or(session_item.command.clone());
        session_item.confidence = session_item.confidence.max(running_item.confidence);
        session_item.status = Some(AgentStatus::Running);
        session_item.last_output = running_item
            .last_output
            .clone()
            .or_else(|| Some("VS Code 集成终端中的 Agent 正在运行。".to_string()));
        session_item.cwd = running_item.cwd.clone().or(session_item.cwd.clone());
        return;
    }

    if let Some(done_item) = child_items
        .iter()
        .find(|item| matches!(item.status, Some(AgentStatus::Done)))
    {
        session_item.kind = done_item.kind.clone();
        session_item.command = done_item.command.clone().or(session_item.command.clone());
        session_item.confidence = session_item.confidence.max(done_item.confidence);
        session_item.status = Some(AgentStatus::Done);
        session_item.last_output = done_item.last_output.clone();
        session_item.cwd = done_item.cwd.clone().or(session_item.cwd.clone());
    } else if child_items.is_empty() {
        session_item.last_output =
            Some("VS Code Remote-SSH 已连接；集成终端中未检测到活动 Agent。".to_string());
    }
}

fn scan_vscode_terminal_process_candidates(
    session: &VscodeRemoteSession,
    machine_label: &str,
    checked_at: &str,
) -> Result<Vec<DiscoveredAgent>, String> {
    let stdout = match vscode_remote_exec_stdout(session, remote_vscode_terminal_agent_script()) {
        Ok(stdout) => stdout,
        Err(_) => {
            let output = ssh_output(&session.endpoint, remote_vscode_terminal_agent_script())
                .map_err(|error| error.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
            }
            String::from_utf8_lossy(&output.stdout).to_string()
        }
    };
    Ok(parse_remote_process_lines(
        &stdout,
        &session.endpoint,
        machine_label,
        "vscode_remote_ssh",
        0.78,
        checked_at,
    ))
}

fn remote_vscode_terminal_agent_script() -> &'static str {
    "ptyhosts=$(ps -eo pid=,args= 2>/dev/null | awk '/bootstrap-fork --type=ptyHost/ && !/awk/ {print $1}'); is_vscode_term_child() { current=\"$1\"; depth=0; while [ -n \"$current\" ] && [ \"$current\" -gt 1 ] 2>/dev/null && [ \"$depth\" -lt 32 ]; do for host in $ptyhosts; do [ \"$current\" = \"$host\" ] && return 0; done; current=$(ps -p \"$current\" -o ppid= 2>/dev/null | tr -d ' '); depth=$((depth + 1)); done; return 1; }; for pid in $(pgrep -f '([c]laude|[c]odex)' 2>/dev/null || true); do is_vscode_term_child \"$pid\" || continue; cmd=$(ps -p \"$pid\" -o args= 2>/dev/null || true); [ -n \"$cmd\" ] || continue; cwd=$(readlink \"/proc/$pid/cwd\" 2>/dev/null || true); runtime=\"\"; if [ -r \"$HOME/.claude/sessions/$pid.json\" ]; then runtime=$(tr '\\n\\t' '  ' < \"$HOME/.claude/sessions/$pid.json\"); fi; printf '%s\\t%s\\t%s\\t%s\\n' \"$pid\" \"$cwd\" \"$cmd\" \"$runtime\"; done"
}

fn vscode_remote_exec_stdout(
    session: &VscodeRemoteSession,
    remote_script: &str,
) -> Result<String, String> {
    let port = session
        .local_forward_port
        .ok_or_else(|| "VS Code Remote-SSH local forwarding port was not found".to_string())?;
    let token = session
        .exec_server_token
        .as_ref()
        .ok_or_else(|| "VS Code Remote-SSH exec server token was not found".to_string())?;
    let electron_node = vscode_electron_node_path()
        .ok_or_else(|| "VS Code Electron Node runtime was not found".to_string())?;
    let helper_path = std::env::temp_dir().join(format!(
        "agent-pilot-vscode-remote-exec-{}.cjs",
        std::process::id()
    ));
    fs::write(&helper_path, VSCODE_REMOTE_EXEC_HELPER)
        .map_err(|error| format!("failed to write VS Code helper: {error}"))?;

    let payload = json!({
        "port": port,
        "token": token,
        "command": "sh",
        "args": ["-c", remote_script],
        "timeoutMs": 12000,
    });
    let output = Command::new(electron_node)
        .env("ELECTRON_RUN_AS_NODE", "1")
        .arg(&helper_path)
        .arg(payload.to_string())
        .output()
        .map_err(|error| format!("failed to run VS Code helper: {error}"))?;
    let _ = fs::remove_file(&helper_path);

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let value = serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|error| format!("invalid VS Code helper output: {error}"))?;
    if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("VS Code helper failed")
            .to_string());
    }
    let result = value
        .get("result")
        .ok_or_else(|| "VS Code helper returned no result".to_string())?;
    let exit_code = result
        .get("exitCode")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if exit_code != 0 {
        let stderr = result
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !stderr.is_empty() {
            return Err(stderr);
        }
    }
    Ok(result
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

fn vscode_electron_node_path() -> Option<&'static str> {
    [
        "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)",
        "/Applications/Visual Studio Code - Insiders.app/Contents/Frameworks/Code - Insiders Helper (Plugin).app/Contents/MacOS/Code - Insiders Helper (Plugin)",
    ]
    .into_iter()
    .find(|path| PathBuf::from(path).exists())
}

fn vscode_remote_session_candidate(
    session: &VscodeRemoteSession,
    label: &str,
    checked_at: &str,
) -> DiscoveredAgent {
    let output = if session.local_forward_port.is_some() && session.exec_server_token.is_some() {
        "VS Code Remote-SSH session detected. Agent Pilot can reuse the VS Code connection to monitor remote Claude/Codex processes."
    } else {
        "VS Code Remote-SSH session detected. Waiting for VS Code exec server details to monitor remote Claude/Codex processes."
    };
    DiscoveredAgent {
        fingerprint: vscode_remote_fingerprint(&session.endpoint),
        agent_id: None,
        name: Some(label.to_string()),
        kind: AgentKind::Other,
        location: AgentLocation::Remote,
        machine_label: label.to_string(),
        cwd: Some(format!(
            "VS Code Remote-SSH · {}",
            endpoint_label(&session.endpoint)
        )),
        command: Some("VS Code Remote-SSH".to_string()),
        last_output: Some(output.to_string()),
        status: Some(AgentStatus::Running),
        pid: Some(session.local_server_pid),
        parent_pid: None,
        terminal_pid: session.code_pid,
        tty: None,
        tmux_session: None,
        tmux_pane: None,
        ssh_host: Some(session.endpoint.host.clone()),
        ssh_user: session.endpoint.user.clone(),
        ssh_port: session.endpoint.port,
        ssh_password_required: false,
        discovery_source: "vscode_remote_ssh".to_string(),
        confidence: 0.72,
        detected_at: checked_at.to_string(),
    }
}

fn discover_vscode_remote_sessions() -> Vec<VscodeRemoteSession> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut sessions = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        let mut pieces = trimmed.split_whitespace();
        let Some(pid_text) = pieces.next() else {
            continue;
        };
        let Some(parent_pid_text) = pieces.next() else {
            continue;
        };
        let command = pieces.collect::<Vec<_>>().join(" ");
        if !command.contains("localServer.js")
            || !command.contains("ms-vscode-remote.remote-ssh")
            || !command.contains("\"sshArgs\"")
        {
            continue;
        }
        let Some(local_server_pid) = pid_text.parse::<u32>().ok() else {
            continue;
        };
        let Some(mut session) = parse_vscode_remote_session(
            local_server_pid,
            parent_pid_text.parse::<u32>().ok(),
            &command,
        ) else {
            continue;
        };
        enrich_vscode_remote_session_from_data_file(&mut session);
        let key = vscode_remote_fingerprint(&session.endpoint);
        if seen.insert(key) {
            sessions.push(session);
        }
    }
    sessions
}

fn parse_vscode_remote_session(
    local_server_pid: u32,
    parent_pid: Option<u32>,
    command: &str,
) -> Option<VscodeRemoteSession> {
    let json_start = command.find('{')?;
    let value = serde_json::from_str::<Value>(&command[json_start..]).ok()?;
    let ssh_args = value
        .get("sshArgs")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let (user, host, port) = extract_ssh_destination_from_args(&ssh_args)?;
    let data_file_path = value
        .get("dataFilePath")
        .and_then(Value::as_str)
        .map(PathBuf::from);

    Some(VscodeRemoteSession {
        local_server_pid,
        code_pid: find_vscode_parent_pid(parent_pid),
        endpoint: SshEndpoint {
            host,
            user,
            port,
            password: None,
        },
        data_file_path,
        socks_port: None,
        remote_port: None,
        exec_server_token: None,
        local_forward_port: None,
    })
}

fn enrich_vscode_remote_session_from_data_file(session: &mut VscodeRemoteSession) {
    let Some(path) = &session.data_file_path else {
        return;
    };
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    session.socks_port = value
        .get("socksPort")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .or(session.socks_port);
    session.remote_port = value
        .get("remoteListeningOn")
        .and_then(|value| value.get("port"))
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .or(session.remote_port);
    session.exec_server_token = value
        .get("execServerToken")
        .and_then(Value::as_str)
        .and_then(non_empty_string)
        .or(session.exec_server_token.clone());
    session.local_forward_port = find_vscode_remote_forward_port(session);
}

fn find_vscode_remote_forward_port(session: &VscodeRemoteSession) -> Option<u16> {
    let log_root = home_path("Library/Application Support/Code/logs")?;
    let mut paths = Vec::new();
    collect_vscode_remote_logs(&log_root, 0, &mut paths);
    paths.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH)
    });
    paths.reverse();

    for path in paths.into_iter().take(48) {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if !vscode_remote_log_matches_session(&text, session) {
            continue;
        }
        if let Some(port) = parse_vscode_remote_forward_port_from_log(&text, session) {
            return Some(port);
        }
    }
    None
}

fn collect_vscode_remote_logs(dir: &PathBuf, depth: usize, paths: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_vscode_remote_logs(&path, depth + 1, paths);
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.contains("Remote - SSH.log") {
            paths.push(path);
        }
    }
}

fn vscode_remote_log_matches_session(text: &str, session: &VscodeRemoteSession) -> bool {
    let host_match = text.contains(&format!("ssh-remote+{}", session.endpoint.host))
        || text.contains(&format!("\"{}\"", session.endpoint.host))
        || text.contains(&format!(" {}", session.endpoint.host));
    let data_file_match = session
        .data_file_path
        .as_ref()
        .map(|path| {
            let path_text = path.to_string_lossy();
            text.contains(path_text.as_ref())
        })
        .unwrap_or(false);
    host_match || data_file_match
}

fn parse_vscode_remote_forward_port_from_log(
    text: &str,
    session: &VscodeRemoteSession,
) -> Option<u16> {
    for line in text.lines().rev() {
        if let Some(remote_port) = session.remote_port {
            let remote_marker = format!("remotePort {remote_port}");
            let socks_matches = session
                .socks_port
                .map(|port| line.contains(&format!("socksPort {port}")))
                .unwrap_or(true);
            if line.contains("Starting forwarding server.")
                && line.contains(&remote_marker)
                && socks_matches
            {
                if let Some(port) = extract_u16_after(line, "local port ") {
                    return Some(port);
                }
            }
        }

        let resolved_marker = format!(
            "Resolved \"ssh-remote+{}\" to \"port ",
            session.endpoint.host
        );
        if line.contains(&resolved_marker) {
            if let Some(port) = extract_u16_after(line, &resolved_marker) {
                return Some(port);
            }
        }

        if line.contains("Resolving exec server at port ") {
            if let Some(port) = extract_u16_after(line, "Resolving exec server at port ") {
                return Some(port);
            }
        }
    }
    None
}

fn extract_u16_after(text: &str, marker: &str) -> Option<u16> {
    let index = text.find(marker)? + marker.len();
    let digits = text[index..]
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u16>().ok()
}

fn scan_remote_tmux_candidates(
    endpoint: &SshEndpoint,
    machine_label: &str,
    discovery_source: &str,
    checked_at: &str,
) -> Result<Vec<DiscoveredAgent>, String> {
    let remote = "tmux list-panes -a -F '#{session_name}|#{pane_index}|#{pane_current_command}|#{pane_current_path}' || true";
    let output = ssh_output(endpoint, remote).map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(parse_tmux_lines(
        &String::from_utf8_lossy(&output.stdout),
        AgentLocation::Remote,
        machine_label,
        Some(&endpoint.host),
        endpoint.user.as_ref(),
        endpoint.port,
        discovery_source,
        checked_at,
    ))
}

fn parse_tmux_lines(
    text: &str,
    location: AgentLocation,
    machine_label: &str,
    ssh_host: Option<&String>,
    ssh_user: Option<&String>,
    ssh_port: Option<u16>,
    discovery_source: &str,
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
                ssh_port,
                ssh_password_required: false,
                discovery_source: discovery_source.to_string(),
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
    let endpoint = endpoint_from_remote_host(host);
    capture_ssh_tmux_output_endpoint(&endpoint, &target)
}

fn capture_ssh_tmux_output_endpoint(endpoint: &SshEndpoint, target: &str) -> Option<String> {
    let remote = format!(
        "tmux capture-pane -p -S -80 -t {} || true",
        shell_quote(target)
    );
    let output = ssh_output(endpoint, &remote).ok()?;
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

fn contains_waiting_input_prompt(text: &str) -> bool {
    let lowered = text.to_lowercase();
    if [
        "new task? /clear",
        "press enter to close",
        "ready for input",
        "waiting for input",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        return true;
    }

    text.lines().rev().take(10).any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        let lowered = trimmed.to_lowercase();
        lowered.ends_with(" new task?") || lowered.contains("new task? /")
    })
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
            if let Some(status) = candidate.status.clone() {
                agent.status = status;
            }
            agent.updated_at = now_iso();
            agent.machine_label = candidate.machine_label.clone();
            if candidate.discovery_source == "vscode_remote_ssh" {
                agent.kind = candidate.kind.clone();
                agent.confidence = Some(candidate.confidence);
                agent.current_task = Some(
                    "检测到 VS Code Remote-SSH 会话；正在通过 VS Code 连接通道监看远端 Agent。"
                        .to_string(),
                );
            }
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
            agent.ssh_port = candidate.ssh_port.or(agent.ssh_port);
            agent.ssh_password_required = candidate.ssh_password_required;
            if candidate.discovery_source == "local_ssh" {
                agent
                    .discovery_sources
                    .retain(|source| source != "local_ssh_tmux");
            }
            agent.terminal_target = Some(if should_use_process_terminal_target(candidate) {
                build_process_terminal_target(candidate)
            } else if candidate.discovery_source == "codex_desktop" {
                build_desktop_app_target(candidate)
            } else if is_vscode_remote_discovery_source(&candidate.discovery_source) {
                build_vscode_remote_target(candidate)
            } else if should_use_remote_process_terminal_target(candidate) {
                build_remote_process_terminal_target(
                    candidate,
                    agent
                        .terminal_target
                        .as_ref()
                        .map(|target| target.terminal_app.as_str())
                        .unwrap_or("ghostty"),
                )
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
                    candidate.ssh_port.or(agent.ssh_port),
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
    remote_hosts: &[RemoteHost],
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
    let endpoint = SshEndpoint {
        host: ssh_host.clone(),
        user: ssh_user.clone(),
        port: ssh_port,
        password: matching_remote_host_password(
            remote_hosts,
            ssh_user.as_deref(),
            &ssh_host,
            ssh_port,
        ),
    };
    let mut kind = session
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
    let mut remote_agent_cwd = None;
    let captured_output = session
        .as_deref()
        .and_then(|session| capture_ssh_tmux_output_endpoint(&endpoint, session))
        .or_else(|| tty.as_deref().and_then(capture_terminal_tab_text_by_tty));

    let mut status = None;
    let mut last_output = None;
    let mut remote_scan_failed = false;
    if session.is_none() {
        if let Ok(remote_agents) =
            scan_remote_process_candidates(&endpoint, &remote_label, "local_ssh", 0.72, checked_at)
        {
            if let Some(active_agent) = prioritized_remote_agent(&remote_agents) {
                kind = active_agent.kind.clone();
                remote_agent_cwd = active_agent.cwd.clone();
                status = active_agent.status.clone();
                last_output = active_agent
                    .last_output
                    .clone()
                    .or(active_agent.command.clone());
            }
        } else {
            remote_scan_failed = true;
        }
    }

    let (text_status, text_last_output) = captured_output
        .as_deref()
        .map(|output| infer_status_from_terminal_text(output, Some(command)))
        .unwrap_or((None, None));
    if text_status.is_some() {
        status = text_status;
    }
    if text_last_output.is_some() {
        last_output = text_last_output;
    }
    let ssh_password_required = endpoint.password.is_none()
        && ((session.is_none() && remote_scan_failed)
            || (session.is_some() && captured_output.is_none()))
        && ssh_command_needs_password(&endpoint);

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
            .or(remote_agent_cwd)
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
        ssh_port,
        ssh_password_required,
        discovery_source: "local_ssh".to_string(),
        confidence: 0.86,
        detected_at: checked_at.to_string(),
    })
}

fn matching_remote_host_password(
    remote_hosts: &[RemoteHost],
    ssh_user: Option<&str>,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> Option<String> {
    remote_hosts.iter().find_map(|host| {
        if !remote_host_matches_endpoint(host, ssh_user, ssh_host, ssh_port) {
            return None;
        }
        host.ssh_password.as_deref().and_then(non_empty_string)
    })
}

fn remote_host_matches_endpoint(
    host: &RemoteHost,
    ssh_user: Option<&str>,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> bool {
    if host.ssh_host != ssh_host {
        return false;
    }
    if let Some(user) = ssh_user {
        if !host.ssh_user.is_empty() && host.ssh_user != user {
            return false;
        }
    }
    if let Some(port) = ssh_port {
        if host.ssh_port != port {
            return false;
        }
    }
    true
}

fn endpoint_matches_agent(
    agent: &AgentItem,
    ssh_user: Option<&str>,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> bool {
    endpoint_parts_match(
        agent.ssh_user.as_deref(),
        agent.ssh_host.as_deref(),
        agent.ssh_port,
        ssh_user,
        ssh_host,
        ssh_port,
    )
}

fn endpoint_matches_candidate(
    candidate: &DiscoveredAgent,
    ssh_user: Option<&str>,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> bool {
    endpoint_parts_match(
        candidate.ssh_user.as_deref(),
        candidate.ssh_host.as_deref(),
        candidate.ssh_port,
        ssh_user,
        ssh_host,
        ssh_port,
    )
}

fn endpoint_parts_match(
    item_user: Option<&str>,
    item_host: Option<&str>,
    item_port: Option<u16>,
    ssh_user: Option<&str>,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> bool {
    if item_host != Some(ssh_host) {
        return false;
    }
    if let Some(user) = ssh_user {
        if item_user.unwrap_or_default() != user {
            return false;
        }
    }
    if let Some(port) = ssh_port {
        if item_port.unwrap_or(22) != port {
            return false;
        }
    }
    true
}

fn config_id_fragment(value: &str) -> String {
    let mut out = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

fn prioritized_remote_agent(items: &[DiscoveredAgent]) -> Option<&DiscoveredAgent> {
    items
        .iter()
        .find(|item| matches!(item.status, Some(AgentStatus::WaitingAttention)))
        .or_else(|| {
            items
                .iter()
                .find(|item| matches!(item.status, Some(AgentStatus::Running)))
        })
        .or_else(|| {
            items
                .iter()
                .find(|item| matches!(item.status, Some(AgentStatus::Done)))
        })
        .or_else(|| items.first())
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
    extract_ssh_destination_from_tokens(&tokens.iter().skip(1).copied().collect::<Vec<_>>())
}

fn extract_ssh_destination_from_args(
    args: &[String],
) -> Option<(Option<String>, String, Option<u16>)> {
    extract_ssh_destination_from_tokens(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

fn extract_ssh_destination_from_tokens(
    tokens: &[&str],
) -> Option<(Option<String>, String, Option<u16>)> {
    let mut ssh_user = None;
    let mut ssh_host = None;
    let mut ssh_port = None;
    let mut skip_next = false;
    let options_with_values = [
        "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o",
        "-p", "-Q", "-R", "-S", "-W", "-w",
    ];

    for (index, token) in tokens.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }

        let token = *token;

        if token == "--" {
            continue;
        }

        if token == "-l" {
            ssh_user = tokens.get(index + 1).map(|value| clean_shell_token(value));
            skip_next = true;
            continue;
        }

        if let Some(user_text) = token.strip_prefix("-l").filter(|value| !value.is_empty()) {
            ssh_user = Some(clean_shell_token(user_text));
            continue;
        }

        if token == "-p" {
            ssh_port = tokens
                .get(index + 1)
                .and_then(|value| clean_shell_token(value).parse::<u16>().ok());
            skip_next = true;
            continue;
        }

        if let Some(port_text) = token.strip_prefix("-p") {
            if !port_text.is_empty() {
                ssh_port = port_text.parse::<u16>().ok();
                continue;
            }
        }

        if options_with_values.iter().any(|option| *option == token) {
            skip_next = true;
            continue;
        }

        if is_attached_ssh_option_with_value(token) {
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

fn is_attached_ssh_option_with_value(token: &str) -> bool {
    [
        "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-m", "-O", "-o", "-Q",
        "-R", "-S", "-W", "-w",
    ]
    .iter()
    .any(|option| token.starts_with(option) && token.len() > option.len())
}

fn clean_shell_token(value: &str) -> String {
    value
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(';')
        .to_string()
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
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
    } else if is_vscode_remote_discovery_source(&candidate.discovery_source) {
        build_vscode_remote_target(&candidate)
    } else if should_use_remote_process_terminal_target(&candidate) {
        build_remote_process_terminal_target(&candidate, "ghostty")
    } else {
        build_terminal_target(
            &candidate.location,
            "ghostty",
            candidate.cwd.clone(),
            Some(session_name.clone()),
            candidate.ssh_host.clone(),
            candidate.ssh_user.clone(),
            candidate.ssh_port,
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
        } else if candidate.discovery_source == "vscode_remote_ssh" {
            "检测到 VS Code Remote-SSH 会话；正在通过 VS Code 连接通道监看远端 Agent。".to_string()
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
                || terminal_target.target_type == "ssh-process"
            {
                None
            } else {
                Some(session_name)
            }
        }),
        tmux_pane: candidate.tmux_pane,
        ssh_host: candidate.ssh_host,
        ssh_user: candidate.ssh_user,
        ssh_port: candidate.ssh_port,
        ssh_password_required: candidate.ssh_password_required,
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
            vscode_uri: None,
            ghostty_terminal_id: None,
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
                vscode_uri: None,
                ghostty_terminal_id: None,
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
        vscode_uri: None,
        ghostty_terminal_id: None,
    }
}

fn build_vscode_remote_target(candidate: &DiscoveredAgent) -> TerminalTarget {
    TerminalTarget {
        target_type: "desktop-app".to_string(),
        terminal_app: "vscode".to_string(),
        process_pid: candidate.terminal_pid,
        terminal_pid: candidate.terminal_pid,
        tty: None,
        process_command: candidate.command.clone(),
        ssh_host: candidate.ssh_host.clone(),
        ssh_user: candidate.ssh_user.clone(),
        ssh_port: candidate.ssh_port,
        session_name: Some(
            candidate
                .ssh_host
                .clone()
                .unwrap_or_else(|| "VS Code Remote-SSH".to_string()),
        ),
        local_command: None,
        remote_command: None,
        vscode_uri: vscode_remote_uri_from_candidate(candidate),
        ghostty_terminal_id: None,
    }
}

fn build_remote_process_terminal_target(
    candidate: &DiscoveredAgent,
    terminal_app: &str,
) -> TerminalTarget {
    let pid = candidate.pid.unwrap_or_default();
    let cwd = candidate.cwd.clone().unwrap_or_else(|| "~".to_string());
    let mut remote_command = format!(
        "cd {} 2>/dev/null || true; ps -p {} -o pid,ppid,tty,stat,args",
        shell_quote(&cwd),
        pid
    );
    if let Some(command) = candidate
        .command
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        remote_command.push_str(&format!(
            "; printf '%s\\n' {} {}",
            shell_quote("Command:"),
            shell_quote(command)
        ));
    }
    remote_command.push_str("; printf '\\nPress enter to close...'; read -r _");

    TerminalTarget {
        target_type: "ssh-process".to_string(),
        terminal_app: terminal_app.to_string(),
        process_pid: candidate.pid,
        terminal_pid: None,
        tty: None,
        process_command: candidate.command.clone(),
        ssh_host: candidate.ssh_host.clone(),
        ssh_user: candidate.ssh_user.clone(),
        ssh_port: candidate.ssh_port,
        session_name: Some(format!("{}_pid_{pid}", kind_key(&candidate.kind))),
        local_command: None,
        remote_command: Some(remote_command),
        vscode_uri: None,
        ghostty_terminal_id: None,
    }
}

fn terminal_command(target: &TerminalTarget) -> String {
    if target.target_type == "ssh-tmux" || target.target_type == "ssh-process" {
        let Some(host) = target.ssh_host.clone() else {
            return String::new();
        };
        let ssh_target = ssh_target(target.ssh_user.as_deref(), &host);
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
        let port_arg = target
            .ssh_port
            .map(|port| format!("-p {port} "))
            .unwrap_or_default();
        format!("ssh {port_arg}{ssh_target} -t {}", shell_quote(&remote))
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
    if target.terminal_app == "vscode" {
        if let Some(uri) = target
            .vscode_uri
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            if Command::new("open")
                .arg(uri)
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
            {
                return true;
            }
        }
    }

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
        "vscode" => "Visual Studio Code",
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
    let vscode_uri = if terminal_app == "vscode" {
        vscode_file_uri(candidate.cwd.as_deref())
    } else {
        None
    };
    let ghostty_terminal_id = if terminal_app == "ghostty" {
        find_matching_ghostty_terminal_id(
            candidate.command.as_deref(),
            candidate.cwd.as_deref(),
            candidate.tmux_session.as_deref(),
        )
    } else {
        None
    };
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
        session_name: Some(
            candidate
                .tmux_session
                .clone()
                .unwrap_or_else(|| format!("{}_pid_{pid}", kind_key(&candidate.kind))),
        ),
        local_command: Some(local_process_status_command(
            pid,
            &tty_label,
            candidate.command.as_deref(),
        )),
        remote_command: None,
        vscode_uri,
        ghostty_terminal_id,
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
    if target.terminal_app == "vscode" && focus_desktop_app(target) {
        return true;
    }

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
    if target.terminal_app == "ghostty" {
        return false;
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
        "vscode" => "Visual Studio Code",
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

fn ghostty_terminals() -> Vec<GhosttyTerminal> {
    let script = r#"tell application "Ghostty"
  set rows to {}
  set fieldDelimiter to character id 9
  repeat with ghostTerminal in terminals
    set terminalId to id of ghostTerminal as text
    set terminalName to name of ghostTerminal as text
    set terminalCwd to working directory of ghostTerminal as text
    set end of rows to terminalId & fieldDelimiter & terminalName & fieldDelimiter & terminalCwd
  end repeat
  set oldDelimiters to AppleScript's text item delimiters
  set AppleScript's text item delimiters to linefeed
  set joinedRows to rows as text
  set AppleScript's text item delimiters to oldDelimiters
  return joinedRows
end tell"#;

    let mut command = Command::new("osascript");
    command.args(["-e", script]);
    let Some(output) = command_output_with_timeout(&mut command, Duration::from_millis(1400))
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let id = non_empty_string(parts.next().unwrap_or_default())?;
            let name = parts.next().and_then(non_empty_string)?;
            let working_directory = parts.next().and_then(non_empty_string).unwrap_or_default();
            Some(GhosttyTerminal {
                id,
                name,
                working_directory,
            })
        })
        .collect()
}

fn command_output_with_timeout(command: &mut Command, timeout: Duration) -> Option<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let started_at = SystemTime::now();
    loop {
        if child.try_wait().ok().flatten().is_some() {
            return child.wait_with_output().ok();
        }
        if started_at
            .elapsed()
            .ok()
            .map(|elapsed| elapsed >= timeout)
            .unwrap_or(true)
        {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(40));
    }
}

fn find_matching_ghostty_terminal_id(
    process_command: Option<&str>,
    cwd: Option<&str>,
    tmux_session: Option<&str>,
) -> Option<String> {
    let terminals = ghostty_terminals();
    let title_matches = terminals
        .iter()
        .filter(|terminal| ghostty_terminal_matches(terminal, process_command, None, tmux_session))
        .collect::<Vec<_>>();
    if title_matches.len() == 1 {
        return title_matches.first().map(|terminal| terminal.id.clone());
    }

    let cwd = cwd.map(str::trim).filter(|value| !value.is_empty())?;
    let cwd_matches = terminals
        .into_iter()
        .filter(|terminal| terminal.working_directory == cwd)
        .collect::<Vec<_>>();
    if cwd_matches.len() == 1 {
        cwd_matches.first().map(|terminal| terminal.id.clone())
    } else {
        None
    }
}

fn ghostty_terminal_matches(
    terminal: &GhosttyTerminal,
    process_command: Option<&str>,
    cwd: Option<&str>,
    tmux_session: Option<&str>,
) -> bool {
    let title = normalize_match_text(&terminal.name);
    let command = process_command
        .map(normalize_match_text)
        .unwrap_or_default();
    if !command.is_empty() && title.contains(&command) {
        return true;
    }

    if let Some(session) = tmux_session.map(normalize_match_text) {
        if !session.is_empty() && title.contains(&session) && title.contains("tmux") {
            return true;
        }
    }

    if let Some(command_text) = process_command {
        let tokens = command_text.split_whitespace().collect::<Vec<_>>();
        if let Some((_, host, _)) = extract_ssh_destination(&tokens) {
            if title.contains(&normalize_match_text(&host)) {
                return tmux_session
                    .map(|session| title.contains(&normalize_match_text(session)))
                    .unwrap_or(true);
            }
        }
    }

    cwd.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| terminal.working_directory == value)
        .unwrap_or(false)
}

fn normalize_match_text(value: &str) -> String {
    value
        .replace(['"', '\'', '\\'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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

    let mut command = Command::new("osascript");
    command.args(["-e", &script]);
    command_output_with_timeout(&mut command, Duration::from_millis(1400))
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

fn focus_ghostty_terminal_by_title(target: &TerminalTarget) -> bool {
    if let Some(terminal_id) = target
        .ghostty_terminal_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if focus_ghostty_terminal_by_id(terminal_id) {
            return true;
        }
    }
    let tmux_session = target
        .session_name
        .as_deref()
        .filter(|session| !session.contains("_pid_"))
        .map(str::to_string)
        .or_else(|| {
            target
                .process_command
                .as_deref()
                .and_then(tmux_session_from_process_command)
        });
    if let Some(terminal_id) = find_matching_ghostty_terminal_id(
        target.process_command.as_deref(),
        None,
        tmux_session.as_deref(),
    ) {
        if focus_ghostty_terminal_by_id(&terminal_id) {
            return true;
        }
    }
    if let Some((ssh_host, session)) = target
        .process_command
        .as_deref()
        .and_then(|command| ghostty_ssh_tmux_focus_terms(command, tmux_session.as_deref()))
    {
        if focus_ghostty_terminal_by_terms(&ssh_host, &session) {
            return true;
        }
    }

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

    let mut command = Command::new("osascript");
    command.args(["-e", &script]);
    command_output_with_timeout(&mut command, Duration::from_millis(1400))
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

fn ghostty_ssh_tmux_focus_terms(
    process_command: &str,
    tmux_session: Option<&str>,
) -> Option<(String, String)> {
    let tokens = process_command.split_whitespace().collect::<Vec<_>>();
    let (_, host, _) = extract_ssh_destination(&tokens)?;
    let session = tmux_session
        .and_then(non_empty_string)
        .or_else(|| tmux_session_from_process_command(process_command))?;
    Some((host, session))
}

fn focus_ghostty_terminal_by_terms(ssh_host: &str, tmux_session: &str) -> bool {
    let script = format!(
        r#"tell application "Ghostty"
  set matchedTerminal to missing value
  set matchCount to 0
  repeat with ghostTerminal in terminals
    set terminalTitle to name of ghostTerminal as text
    ignoring case
      if terminalTitle contains {ssh_host} and terminalTitle contains {tmux_session} then
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
        ssh_host = apple_quote(ssh_host),
        tmux_session = apple_quote(tmux_session)
    );

    let mut command = Command::new("osascript");
    command.args(["-e", &script]);
    command_output_with_timeout(&mut command, Duration::from_millis(1400))
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

fn tmux_session_from_process_command(command: &str) -> Option<String> {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let tmux_index = tokens.iter().position(|token| {
        token
            .trim_matches('"')
            .trim_matches('\'')
            .rsplit('/')
            .next()
            .map(|name| name == "tmux")
            .unwrap_or(false)
    })?;
    extract_tmux_session(&tokens[tmux_index + 1..])
}

fn focus_ghostty_terminal_by_id(terminal_id: &str) -> bool {
    let script = format!(
        r#"tell application "Ghostty"
  repeat with ghostTerminal in terminals
    if (id of ghostTerminal as text) is {terminal_id} then
      focus ghostTerminal
      return true
    end if
  end repeat
end tell
return false"#,
        terminal_id = apple_quote(terminal_id)
    );

    let mut command = Command::new("osascript");
    command.args(["-e", &script]);
    command_output_with_timeout(&mut command, Duration::from_millis(1400))
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

fn open_local_process_terminal(target: &TerminalTarget) -> bool {
    if target.terminal_pid.is_some() && target.tty.is_some() && focus_local_process_terminal(target)
    {
        return true;
    }
    if target.terminal_app == "ghostty" {
        return false;
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
            || is_vscode_terminal_host_command(&lowered)
        {
            return Some(current);
        }
        pid = process_parent_and_command(current).map(|(parent, _)| parent);
    }
    None
}

fn find_vscode_parent_pid(mut pid: Option<u32>) -> Option<u32> {
    for _ in 0..10 {
        let current = pid?;
        let (_, command) = process_parent_and_command(current)?;
        let lowered = command.to_lowercase();
        if lowered.contains("visual studio code.app/contents/macos/code") {
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
    } else if is_vscode_terminal_host_command(&lowered) {
        Some("vscode".to_string())
    } else {
        None
    }
}

fn is_vscode_terminal_host_command(lowered_command: &str) -> bool {
    lowered_command.contains("visual studio code.app")
        && lowered_command.contains("code helper")
        && lowered_command.contains("node.mojom.nodeservice")
}

fn should_use_process_terminal_target(candidate: &DiscoveredAgent) -> bool {
    candidate.discovery_source == "local_ssh"
        || candidate.discovery_source == "local_ssh_tmux"
        || (candidate.discovery_source == "local_process"
            && matches!(candidate.location, AgentLocation::Local)
            && candidate.tmux_session.is_none())
}

fn should_use_remote_process_terminal_target(candidate: &DiscoveredAgent) -> bool {
    matches!(candidate.location, AgentLocation::Remote)
        && candidate.pid.is_some()
        && candidate.tmux_session.is_none()
        && matches!(candidate.discovery_source.as_str(), "remote_process")
}

fn is_vscode_remote_discovery_source(source: &str) -> bool {
    matches!(source, "vscode_remote_ssh")
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
    if agent
        .discovery_sources
        .iter()
        .any(|source| source == "vscode_remote_process" || source == "vscode_remote_tmux")
        && !agent
            .discovery_sources
            .iter()
            .any(|source| source == "vscode_remote_ssh")
    {
        fingerprints.push(format!("manual:{}", agent.id));
        return fingerprints;
    }
    if agent
        .discovery_sources
        .iter()
        .any(|source| source == "vscode_remote_ssh")
    {
        if let Some(host) = agent.ssh_host.clone() {
            let endpoint = SshEndpoint {
                host,
                user: agent.ssh_user.clone(),
                port: agent.ssh_port,
                password: None,
            };
            fingerprints.push(vscode_remote_fingerprint(&endpoint));
        }
        if let Some(pid) = agent.pid {
            fingerprints.push(format!(
                "vscode-remote-ssh:{}@{}:{:?}:{}",
                agent.ssh_user.clone().unwrap_or_default(),
                agent.ssh_host.clone().unwrap_or_default(),
                agent.ssh_port,
                pid
            ));
        }
    }
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
                fingerprints.push(format!(
                    "remote:{}@{}:{}:{}",
                    agent.ssh_user.clone().unwrap_or_default(),
                    agent.ssh_host.clone().unwrap_or_default(),
                    kind_key(kind),
                    pid
                ));
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
    for host in &mut config.remote_hosts {
        let is_ui_saved_credential = host.id.starts_with("ssh-")
            && host
                .ssh_password
                .as_deref()
                .and_then(non_empty_string)
                .is_some();
        if is_ui_saved_credential {
            host.scan_enabled = false;
        }
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
            if agent
                .discovery_sources
                .iter()
                .any(|source| source == "vscode_remote_process" || source == "vscode_remote_tmux")
                && !agent
                    .discovery_sources
                    .iter()
                    .any(|source| source == "vscode_remote_ssh")
            {
                return false;
            }

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

fn is_agent_pilot_internal_ssh_command(command: &str) -> bool {
    let lowered = command.to_lowercase();
    lowered.contains("numberofpasswordprompts=1")
        && lowered.contains("stricthostkeychecking=accept-new")
        && (lowered.contains("tmux capture-pane")
            || lowered.contains("pgrep -f")
            || lowered.contains("bootstrap-fork --type=ptyhost"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vscode_remote_ssh_destination() {
        let args = [
            "-v",
            "-T",
            "-D",
            "61160",
            "-o",
            "ConnectTimeout=15",
            "172.16.4.25-ct",
        ]
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();

        assert_eq!(
            extract_ssh_destination_from_args(&args),
            Some((None, "172.16.4.25-ct".to_string(), None))
        );
    }

    #[test]
    fn parses_explicit_ssh_user_and_port() {
        let tokens = ["ssh", "-t", "root@172.16.4.25", "-p", "30291", "tmux"];

        assert_eq!(
            extract_ssh_destination(&tokens),
            Some((
                Some("root".to_string()),
                "172.16.4.25".to_string(),
                Some(30291)
            ))
        );
    }

    #[test]
    fn maps_claude_waiting_session_to_attention() {
        let hint = parse_claude_runtime_hint(
            r#"{"pid":59600,"status":"waiting","waitingFor":"approve Bash","cwd":"/tmp/project"}"#,
        )
        .expect("runtime hint");

        assert!(matches!(hint.status, Some(AgentStatus::WaitingAttention)));
        assert_eq!(
            hint.last_output,
            Some("Claude Code 等待处理：approve Bash".to_string())
        );
        assert_eq!(hint.cwd, Some("/tmp/project".to_string()));
    }

    #[test]
    fn treats_terminal_new_task_prompt_as_weak_idle_marker() {
        let (status, _) = infer_status_from_terminal_text(
            "✔ Create run.py\n❯\n⏵⏵ accept edits on · new task? /clear to save",
            Some("claude"),
        );

        assert!(status.is_none());
        assert!(contains_waiting_input_prompt(
            "✔ Create run.py\n❯\n⏵⏵ accept edits on · new task? /clear to save"
        ));
    }

    #[test]
    fn bare_agent_input_prompt_is_not_idle_marker() {
        let (status, _) = infer_status_from_terminal_text(
            "Analyzing repository\n› Explain this codebase",
            Some("codex"),
        );

        assert!(status.is_none());
        assert!(!contains_waiting_input_prompt(
            "Analyzing repository\n› Explain this codebase"
        ));
    }

    #[test]
    fn keeps_approval_prompt_as_attention_before_idle_prompt() {
        let (status, _) =
            infer_status_from_terminal_text("Do you want to proceed?\n› yes", Some("codex"));

        assert!(matches!(status, Some(AgentStatus::WaitingAttention)));
    }

    #[test]
    fn parses_vscode_remote_forward_port_from_log() {
        let session = VscodeRemoteSession {
            local_server_pid: 59914,
            code_pid: Some(710),
            endpoint: SshEndpoint {
                host: "172.16.4.25-ct".to_string(),
                user: None,
                port: None,
                password: None,
            },
            data_file_path: None,
            socks_port: Some(61160),
            remote_port: Some(50703),
            exec_server_token: Some("token".to_string()),
            local_forward_port: None,
        };
        let log = r#"
[11:18:20.503] Starting forwarding server. local port 61233 -> socksPort 61160 -> remotePort 50703
[11:18:20.504] Resolved "ssh-remote+172.16.4.25-ct" to "port 61233"
"#;

        assert_eq!(
            parse_vscode_remote_forward_port_from_log(log, &session),
            Some(61233)
        );
    }

    #[test]
    fn builds_stable_vscode_remote_fingerprint_without_local_server_pid() {
        let endpoint = SshEndpoint {
            host: "172.16.4.25-ct".to_string(),
            user: None,
            port: None,
            password: None,
        };

        assert_eq!(
            vscode_remote_fingerprint(&endpoint),
            "vscode-remote-ssh:@172.16.4.25-ct:None"
        );
    }

    #[test]
    fn builds_vscode_remote_uri_for_remote_workspace() {
        let endpoint = SshEndpoint {
            host: "172.16.4.25".to_string(),
            user: Some("root".to_string()),
            port: Some(30291),
            password: None,
        };

        assert_eq!(
            vscode_remote_uri(&endpoint, Some("/root/my project")),
            "vscode://vscode-remote/ssh-remote+root%40172.16.4.25/root/my%20project"
        );
    }

    #[test]
    fn builds_vscode_file_uri_for_local_workspace() {
        assert_eq!(
            vscode_file_uri(Some("/Users/van/My Project")),
            Some("vscode://file/Users/van/My%20Project".to_string())
        );
    }

    #[test]
    fn matches_ghostty_ssh_tmux_title_with_quoted_remote_command() {
        let terminal = GhosttyTerminal {
            id: "terminal-id".to_string(),
            name: r#"ssh -t root@172.16.4.25 -p 30291 "tmux attach-session -t codex""#.to_string(),
            working_directory: "/Users/van".to_string(),
        };

        assert!(ghostty_terminal_matches(
            &terminal,
            Some("ssh -t root@172.16.4.25 -p 30291 tmux attach-session -t codex"),
            None,
            Some("codex")
        ));
    }

    #[test]
    fn extracts_tmux_session_from_ssh_process_command() {
        assert_eq!(
            tmux_session_from_process_command(
                "ssh -t root@172.16.4.25 -p 30291 tmux attach-session -t codex"
            ),
            Some("codex".to_string())
        );
    }

    #[test]
    fn builds_ghostty_focus_terms_from_ssh_tmux_command() {
        assert_eq!(
            ghostty_ssh_tmux_focus_terms(
                "ssh -t root@172.16.4.25 -p 30291 tmux attach-session -t codex",
                None
            ),
            Some(("172.16.4.25".to_string(), "codex".to_string()))
        );
    }

    #[test]
    fn matches_local_claude_ghostty_title_before_shared_cwd() {
        let terminal = GhosttyTerminal {
            id: "claude-terminal".to_string(),
            name: "✳ Claude Code".to_string(),
            working_directory: "/Users/van".to_string(),
        };

        assert!(ghostty_terminal_matches(
            &terminal,
            Some("claude"),
            None,
            None
        ));
    }

    #[test]
    fn finds_saved_password_for_matching_remote_host() {
        let hosts = vec![RemoteHost {
            id: "server-main".to_string(),
            label: "Linux Server".to_string(),
            ssh_host: "172.16.4.25".to_string(),
            ssh_user: "root".to_string(),
            ssh_port: 30291,
            ssh_password: Some("secret".to_string()),
            scan_enabled: true,
        }];

        assert_eq!(
            matching_remote_host_password(&hosts, Some("root"), "172.16.4.25", Some(30291)),
            Some("secret".to_string())
        );
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
