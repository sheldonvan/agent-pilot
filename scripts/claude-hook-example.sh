#!/usr/bin/env bash
set -euo pipefail

curl -sS -X POST "http://127.0.0.1:8787/api/events" \
  -H "Content-Type: application/json" \
  -d "{
    \"agentId\": \"local-claude-${USER}\",
    \"name\": \"Local Claude Code\",
    \"kind\": \"claude_code\",
    \"location\": \"local\",
    \"machineLabel\": \"Local Mac\",
    \"cwd\": \"${PWD}\",
    \"status\": \"running\",
    \"currentTask\": \"Claude Code session started\"
  }"
