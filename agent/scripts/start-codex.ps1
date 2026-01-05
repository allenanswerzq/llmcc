# Start Codex CLI with Copilot API Bridge
# Usage: .\scripts\start-codex.ps1

$env:OPENAI_BASE_URL = "http://localhost:5168/v1"
$env:OPENAI_API_KEY = "dummy"

Write-Host "Starting Codex with Copilot API Bridge..." -ForegroundColor Cyan
Write-Host "  OPENAI_BASE_URL = $env:OPENAI_BASE_URL" -ForegroundColor Gray
Write-Host ""

codex
