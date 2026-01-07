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
# Only set API_KEY, not AUTH_TOKEN to avoid conflict
unset ANTHROPIC_AUTH_TOKEN

echo -e "\033[36mStarting Claude Code with Copilot API Bridge...\033[0m"
echo -e "\033[90m  ANTHROPIC_BASE_URL = $ANTHROPIC_BASE_URL\033[0m"
echo ""

# Use --dangerously-skip-permissions to allow Claude to read files and run commands
# Remove this flag in production and use proper permission grants instead
# Use --model opus for extended thinking capability
# Use --verbose to show more internal details
claude --dangerously-skip-permissions --model opus --verbose "$@"
