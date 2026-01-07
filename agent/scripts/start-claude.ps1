# Start Claude Code with Copilot API Bridge
# Usage: .\scripts\start-claude.ps1 [--chrome] [other claude args...]
#
# The Copilot API Bridge extension must be running in VS Code before using this script.
# Start the bridge: Cmd/Ctrl+Shift+P -> "Copilot API Bridge: Start Server"

# Configure Claude Code to use the Copilot API Bridge
$env:ANTHROPIC_BASE_URL = "http://localhost:5168"
# Use ANTHROPIC_AUTH_TOKEN (preferred for gateways) - bridge accepts any value
$env:ANTHROPIC_AUTH_TOKEN = "sk-copilot-bridge"
# Also set API_KEY as fallback
$env:ANTHROPIC_API_KEY = "sk-copilot-bridge"

Write-Host "Starting Claude Code with Copilot API Bridge..." -ForegroundColor Cyan
Write-Host "  ANTHROPIC_BASE_URL = $env:ANTHROPIC_BASE_URL" -ForegroundColor Gray
Write-Host "  ANTHROPIC_AUTH_TOKEN = sk-copilot-bridge" -ForegroundColor Gray
Write-Host ""

# Check if --chrome flag is present
if ($args -contains "--chrome") {
    Write-Host "Note: --chrome requires Claude in Chrome extension to be installed." -ForegroundColor Yellow
    Write-Host "The bridge handles LLM calls; Chrome extension provides browser tools." -ForegroundColor Yellow
    Write-Host ""
}

# Use --dangerously-skip-permissions to allow Claude to read files and run commands
# Remove this flag in production and use proper permission grants instead
# Use --model opus for extended thinking capability
# Use --verbose to show more internal details
claude --dangerously-skip-permissions --model opus --verbose @args
