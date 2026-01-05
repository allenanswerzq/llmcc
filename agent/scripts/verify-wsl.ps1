# Verify WSL connectivity to Copilot API Bridge
# Run from Windows PowerShell

Write-Host ""
Write-Host "=== Copilot API Bridge - WSL Verification ===" -ForegroundColor Cyan

# Check 1: Server binding
Write-Host ""
Write-Host "[1/4] Checking server binding..." -ForegroundColor Yellow
$binding = netstat -an | Select-String "5168" | Select-String "LISTENING"
if ($binding -match "0\.0\.0\.0:5168") {
    Write-Host "  OK: Server bound to 0.0.0.0:5168" -ForegroundColor Green
} elseif ($binding -match "127\.0\.0\.1:5168") {
    Write-Host "  FAIL: Server bound to 127.0.0.1 only (WSL will not work)" -ForegroundColor Red
    Write-Host "    Set copilot-api-bridge.bindAddress to 0.0.0.0" -ForegroundColor Gray
} else {
    Write-Host "  FAIL: Server not running on port 5168" -ForegroundColor Red
    exit 1
}

# Check 2: Windows connectivity
Write-Host ""
Write-Host "[2/4] Testing Windows localhost..." -ForegroundColor Yellow
try {
    $response = Invoke-RestMethod -Uri "http://127.0.0.1:5168/v1/models" -TimeoutSec 5
    $count = $response.data.Count
    Write-Host "  OK: Windows - $count models available" -ForegroundColor Green
} catch {
    Write-Host "  FAIL: Cannot connect from Windows" -ForegroundColor Red
}

# Check 3: WSL gateway IP
Write-Host ""
Write-Host "[3/4] Detecting WSL gateway..." -ForegroundColor Yellow
$wslIP = (wsl bash -c "ip route show default") -replace ".*via\s+(\d+\.\d+\.\d+\.\d+).*", '$1'
if ($wslIP -match "^\d+\.\d+\.\d+\.\d+$") {
    Write-Host "  OK: WSL gateway is $wslIP" -ForegroundColor Green
} else {
    Write-Host "  FAIL: Could not detect WSL gateway" -ForegroundColor Red
    exit 1
}

# Check 4: WSL connectivity
Write-Host ""
Write-Host "[4/4] Testing WSL connectivity..." -ForegroundColor Yellow
$wslResult = wsl bash -c "curl -s http://${wslIP}:5168/v1/models"
if ($wslResult -match "object") {
    Write-Host "  OK: WSL can connect to API Bridge" -ForegroundColor Green
} else {
    Write-Host "  FAIL: WSL cannot connect" -ForegroundColor Red
    Write-Host "    Try adding firewall rule with admin privileges:" -ForegroundColor Gray
    Write-Host "    netsh advfirewall firewall add rule name=CopilotBridge dir=in action=allow protocol=TCP localport=5168" -ForegroundColor Gray
    exit 1
}

Write-Host ""
Write-Host "=== All checks passed! ===" -ForegroundColor Green
Write-Host "You can now use from WSL:" -ForegroundColor Cyan
Write-Host "  ./scripts/start-claude.sh" -ForegroundColor White
Write-Host "  ./scripts/start-codex.sh" -ForegroundColor White
Write-Host ""
