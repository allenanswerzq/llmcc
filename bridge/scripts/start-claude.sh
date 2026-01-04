#!/bin/bash
# Start Claude Code with Copilot API Bridge
# Usage: ./scripts/start-claude.sh

# Detect if running in WSL and get Windows host IP
if grep -qi microsoft /proc/version 2>/dev/null; then
    # Running in WSL - use the default gateway (Windows host)
    HOST_IP=$(ip route show default | awk '{print $3}')
    echo -e "\033[33mWSL detected, using Windows host: $HOST_IP\033[0m"
else
    HOST_IP="localhost"
fi

export ANTHROPIC_BASE_URL="http://${HOST_IP}:5168"
export ANTHROPIC_API_KEY="sk-copilot-bridge"
export ANTHROPIC_AUTH_TOKEN="sk-copilot-bridge"

echo -e "\033[36mStarting Claude Code with Copilot API Bridge...\033[0m"
echo -e "\033[90m  ANTHROPIC_BASE_URL = $ANTHROPIC_BASE_URL\033[0m"
echo ""

claude "$@"
