# Start Claude Code with Copilot API Bridge
# Usage: .\scripts\start-claude.ps1

$env:ANTHROPIC_BASE_URL = "http://localhost:5168/v1"
$env:ANTHROPIC_API_KEY = "dummy"

Write-Host "Starting Claude Code with Copilot API Bridge..." -ForegroundColor Cyan
Write-Host "  ANTHROPIC_BASE_URL = $env:ANTHROPIC_BASE_URL" -ForegroundColor Gray
Write-Host ""

# Use --dangerously-skip-permissions to allow Claude to read files and run commands
# Remove this flag in production and use proper permission grants instead
claude --dangerously-skip-permissions @args
