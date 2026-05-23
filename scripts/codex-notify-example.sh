#!/usr/bin/env bash
set -euo pipefail

curl -sS -X POST "http://127.0.0.1:8787/api/events" \
  -H "Content-Type: application/json" \
  -d "{
    \"agentId\": \"local-codex-${USER}\",
    \"name\": \"Local Codex CLI\",
    \"kind\": \"codex\",
    \"location\": \"local\",
    \"machineLabel\": \"Local Mac\",
    \"cwd\": \"${PWD}\",
    \"status\": \"running\",
    \"currentTask\": \"Codex CLI session update\"
  }"
