# Agent Pilot

Language: [简体中文](README.md) | English

Agent Pilot is a macOS-first floating control panel for managing multiple AI agents. It currently supports macOS local terminals, Ghostty terminals, SSH sessions, remote server terminals, and tmux windows. If macOS says the app is damaged, run `xattr -dr com.apple.quarantine "/Applications/Agent Pilot.app"` in Terminal.

Too many agents and no clear sense of what each one is doing? Agent Pilot shows each agent's runtime state with color-coded status, making multi-agent monitoring more direct and less noisy. The dmg build is available from the release page. If you like it, a star is appreciated.

<img width="430" height="650" alt="image" src="https://github.com/user-attachments/assets/9836db4d-d456-417d-b272-29ff3ae00aa7" />

Agent Pilot helps monitor, locate, and manage Claude Code, Codex CLI, and other AI agent sessions running in local or remote terminals.

> Current version: v0.2.0
>
> Download: [Agent Pilot v0.2.0](https://github.com/sheldonvan/agent-pilot/releases/tag/v0.2.0)
>
> License: code is licensed under Apache-2.0; product name, icons, screenshots, release artwork, and other brand assets are reserved.

## What Works Today

- Floating list UI with title, counters, discovery notices, agent cards, and a bottom status bar.
- Discovery for startup scans, automatic scans, local processes, local tmux panes, and configured remote SSH/tmux targets.
- VS Code Remote-SSH discovery: detects VS Code Remote-SSH `localServer.js` sessions, resolves SSH aliases, and scans remote `claude/codex` processes and tmux panes when the target supports non-interactive authentication.
- Ghostty / SSH support: detects local agents, SSH agents, and SSH + tmux agents running in Ghostty, then tries to focus the exact terminal session.
- Agent list management: newly detected terminals are added automatically, and discovered agents disappear when their backing terminal or process goes away.
- Candidate agents: incomplete or ambiguous discoveries are kept as candidates so they can be confirmed or ignored.
- Manual agent creation for local/remote targets, Ghostty/Terminal/iTerm2, tmux sessions, and SSH fields.
- One-click terminal opening for local zsh + tmux and remote SSH + tmux targets.
- Status detection that prioritizes approval prompts, Claude runtime state, Codex Desktop logs, tmux pane metadata, and terminal output changes instead of treating every editable input box as idle.
- Collector API: `/api/events`, `/api/state`, `/api/discovery/*`, and `/api/open-terminal`.
- Local configuration at `~/.agent-pilot/config.json`.

## Download

The current release provides a macOS Apple Silicon dmg:

```text
Agent.Pilot_0.2.0_aarch64.dmg
```

Download URL:

```text
https://github.com/sheldonvan/agent-pilot/releases/download/v0.2.0/Agent.Pilot_0.2.0_aarch64.dmg
```

SHA-256:

```text
86771867833e47e40dd71dbf47c0b9376648d430b4eada529b90a4752ca1e21f
```

## Local Preview

```bash
python3 -m http.server 5174 -d web
```

Then open `http://127.0.0.1:5174`.

## Running With Tauri

The frontend does not require npm, pnpm, or yarn. The desktop shell requires the Tauri CLI and a recent Rust toolchain.

Install the current stable Rust toolchain, then install the Tauri CLI:

```bash
cargo install tauri-cli --version "^2"
CARGO_TARGET_DIR=/private/tmp/agent-pilot-run-target cargo tauri dev
```

After startup, the collector listens at:

```text
http://127.0.0.1:8787
```

Main API endpoints:

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

## Remote Scan Configuration

See [examples/config.example.json](examples/config.example.json) for configuring fixed servers in `~/.agent-pilot/config.json`.

VS Code Remote-SSH sessions and local terminal `ssh` sessions can be discovered without manually adding entries to `remoteHosts`. Agent Pilot reads the SSH target from process arguments. If a session requires a password for background scanning, the frontend shows a "needs SSH password" marker next to the agent name; clicking it lets the user enter the password, which is saved locally and used for later background SSH status reads. Passwords entered from an agent card are treated as credentials for that session and do not automatically enable global scanning for the whole server, avoiding a flood of cards for every process on the same host. Public-key authentication and ControlMaster remain the recommended setup.

Ghostty sessions are identified with AppleScript-exposed terminal `id`, `name`, and `working directory` values. For agents launched after direct `ssh`, inside `tmux`, or inside other terminal multiplexers, Agent Pilot prefers remote process scans and available runtime status files. When it can parse a tmux session, it continues to use `tmux capture-pane`.

## Hooks / Wrapper

Claude Code and Codex notify examples:

```bash
scripts/claude-hook-example.sh
scripts/codex-notify-example.sh
```

You can also start any agent through the wrapper:

```bash
PILOT_AGENT_KIND=codex PILOT_AGENT_NAME="Local Codex" scripts/agent-pilot local-codex codex
```

## License and Copyright

Code is licensed under the [Apache License 2.0](LICENSE) unless a file states otherwise. Copyright and attribution notices are in [NOTICE](NOTICE).

The Agent Pilot name, branding, icons, screenshots, release artwork, and other visual brand assets are not licensed for reuse under the code license except as required for fair attribution or to describe the origin of the software.
