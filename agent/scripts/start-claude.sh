#!/bin/bash
# Start Claude Code with Copilot API Bridge
# Usage: ./scripts/start-claude.sh [--chrome] [other claude args...]
#
# The Copilot API Bridge extension must be running in VS Code before using this script.
# Start the bridge: Cmd/Ctrl+Shift+P -> "Copilot API Bridge: Start Server"

set -e

# Detect if running in WSL and get Windows host IP
if grep -qi microsoft /proc/version 2>/dev/null; then
    # Running in WSL - use the default gateway (Windows host)
    HOST_IP=$(ip route show default | awk '{print $3}')
    echo -e "\033[33mWSL detected, using Windows host: $HOST_IP\033[0m"
else
    HOST_IP="localhost"
fi

# Configure Claude Code to use the Copilot API Bridge
export ANTHROPIC_BASE_URL="http://${HOST_IP}:5168"
# Use ANTHROPIC_AUTH_TOKEN (preferred for gateways) - bridge accepts any value
export ANTHROPIC_AUTH_TOKEN="sk-copilot-bridge"
# Also set API_KEY as fallback
export ANTHROPIC_API_KEY="sk-copilot-bridge"

echo -e "\033[36mStarting Claude Code with Copilot API Bridge...\033[0m"
echo -e "\033[90m  ANTHROPIC_BASE_URL = $ANTHROPIC_BASE_URL\033[0m"
echo -e "\033[90m  ANTHROPIC_AUTH_TOKEN = sk-copilot-bridge\033[0m"
echo ""

# Check if --chrome flag is present
CHROME_FLAG=""
for arg in "$@"; do
    if [ "$arg" = "--chrome" ]; then
        CHROME_FLAG="--chrome"
        echo -e "\033[33mNote: --chrome requires Claude in Chrome extension to be installed.\033[0m"
        echo -e "\033[33mThe bridge handles LLM calls; Chrome extension provides browser tools.\033[0m"
        echo ""
        break
    fi
done

# Use --dangerously-skip-permissions to allow Claude to read files and run commands
# Remove this flag in production and use proper permission grants instead
# Use --model opus for extended thinking capability
# Use --verbose to show more internal details
claude --dangerously-skip-permissions --model opus --verbose "$@"
