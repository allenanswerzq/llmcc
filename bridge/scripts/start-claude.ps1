# Start Claude Code with Copilot API Bridge
# Usage: .\scripts\start-claude.ps1

$env:ANTHROPIC_BASE_URL = "http://localhost:5168/v1"
$env:ANTHROPIC_API_KEY = "dummy"

Write-Host "Starting Claude Code with Copilot API Bridge..." -ForegroundColor Cyan
Write-Host "  ANTHROPIC_BASE_URL = $env:ANTHROPIC_BASE_URL" -ForegroundColor Gray
Write-Host ""

claude
