#!/bin/bash
# Start Codex CLI with Copilot API Bridge
# Usage: ./scripts/start-codex.sh

# Detect if running in WSL and get Windows host IP
if grep -qi microsoft /proc/version 2>/dev/null; then
    # Running in WSL - use the default gateway (Windows host)
    HOST_IP=$(ip route show default | awk '{print $3}')
    echo -e "\033[33mWSL detected, using Windows host: $HOST_IP\033[0m"
else
    HOST_IP="localhost"
fi

export OPENAI_BASE_URL="http://${HOST_IP}:5168/v1"
export OPENAI_API_KEY="sk-copilot-bridge"

echo -e "\033[36mStarting Codex with Copilot API Bridge...\033[0m"
echo -e "\033[90m  OPENAI_BASE_URL = $OPENAI_BASE_URL\033[0m"
echo ""

codex "$@"
