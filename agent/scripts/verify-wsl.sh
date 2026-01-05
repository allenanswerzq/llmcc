#!/bin/bash
# Verify WSL connectivity to Copilot API Bridge
# Run from inside WSL

echo ""
echo -e "\033[36m=== Copilot API Bridge - WSL Verification ===\033[0m"

# Check 1: Detect Windows host
echo -e "\n\033[33m[1/3] Detecting Windows host...\033[0m"
HOST_IP=$(ip route show default | awk '{print $3}')
if [ -n "$HOST_IP" ]; then
    echo -e "  \033[32m✓ Windows host: $HOST_IP\033[0m"
else
    echo -e "  \033[31m✗ Could not detect Windows host IP\033[0m"
    exit 1
fi

# Check 2: Test connectivity
echo -e "\n\033[33m[2/3] Testing API Bridge connectivity...\033[0m"
RESPONSE=$(curl -s --connect-timeout 5 "http://${HOST_IP}:5168/v1/models" 2>/dev/null)
if echo "$RESPONSE" | grep -q '"object":"list"'; then
    MODEL_COUNT=$(echo "$RESPONSE" | grep -o '"id"' | wc -l)
    echo -e "  \033[32m✓ Connected! $MODEL_COUNT models available\033[0m"
else
    echo -e "  \033[31m✗ Cannot connect to http://${HOST_IP}:5168\033[0m"
    echo -e "  \033[90m  Make sure VS Code is running with the extension enabled\033[0m"
    echo -e "  \033[90m  Check Windows Firewall allows port 5168\033[0m"
    exit 1
fi

# Check 3: Test a sample request
echo -e "\n\033[33m[3/3] Testing API request...\033[0m"
TEST_RESPONSE=$(curl -s --connect-timeout 10 "http://${HOST_IP}:5168/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{"model":"claude-sonnet-4","messages":[{"role":"user","content":"Say hi"}],"max_tokens":10}' 2>/dev/null)
if echo "$TEST_RESPONSE" | grep -q '"choices"'; then
    echo -e "  \033[32m✓ API request successful\033[0m"
else
    echo -e "  \033[33m⚠ API endpoint reachable but request failed (Copilot may need auth)\033[0m"
fi

echo -e "\n\033[32m=== Verification complete! ===\033[0m"
echo -e "\033[36mYou can now use:\033[0m"
echo -e "  \033[37m./scripts/start-claude.sh\033[0m"
echo -e "  \033[37m./scripts/start-codex.sh\033[0m"
echo -e "\n\033[90mOr manually:\033[0m"
echo -e "  \033[90mexport OPENAI_BASE_URL=\"http://${HOST_IP}:5168/v1\"\033[0m"
echo -e "  \033[90mexport ANTHROPIC_BASE_URL=\"http://${HOST_IP}:5168/v1\"\033[0m"
echo ""
