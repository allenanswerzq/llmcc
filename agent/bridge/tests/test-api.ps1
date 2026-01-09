# Test script for Copilot API Bridge

Write-Host "=== Testing Copilot API Bridge ===" -ForegroundColor Cyan

# Test 1: Health check
Write-Host "`n1. Health Check:" -ForegroundColor Yellow
$health = Invoke-RestMethod -Uri "http://localhost:5168/" -Method Get
$health | ConvertTo-Json

# Test 2: List models
Write-Host "`n2. Available Models:" -ForegroundColor Yellow
$models = Invoke-RestMethod -Uri "http://localhost:5168/v1/models" -Method Get
Write-Host "Found $($models.data.Count) models:"
$models.data | ForEach-Object { Write-Host "  - $($_.id)" }

# Test 3: Chat completion (non-streaming)
Write-Host "`n3. Chat Completion Test:" -ForegroundColor Yellow
$body = @{
    model = "claude-opus-4.5"
    messages = @(
        @{
            role = "user"
            content = "Say hello in exactly 3 words"
        }
    )
    stream = $false
} | ConvertTo-Json -Depth 3

try {
    $response = Invoke-RestMethod -Uri "http://localhost:5168/v1/chat/completions" -Method Post -Body $body -ContentType "application/json"
    Write-Host "Model: $($response.model)"
    Write-Host "Response: $($response.choices[0].message.content)"
    Write-Host "Tokens - Prompt: $($response.usage.prompt_tokens), Completion: $($response.usage.completion_tokens)"
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host "`n=== Tests Complete ===" -ForegroundColor Cyan
