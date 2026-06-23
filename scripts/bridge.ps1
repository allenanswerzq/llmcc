$codexHome = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE ".codex" }
$codexConfig = Join-Path $codexHome "config.toml"
New-Item -ItemType Directory -Path $codexHome -Force | Out-Null

if (-not $env:CODEX_LOCAL_KEY) {
	$env:CODEX_LOCAL_KEY = "dummy"
}

$config = if (Test-Path $codexConfig) {
	Get-Content $codexConfig -Raw
} else {
	""
}

$managedBlock = @"
# BEGIN llmcc local Codex provider
model_provider = "local"
model = "gpt-5.5"

[model_providers.local]
name = "Local Copilot Bridge"
base_url = "http://localhost:5168/v1"
env_key = "CODEX_LOCAL_KEY"
wire_api = "responses"
# END llmcc local Codex provider
"@

$pattern = '(?s)# BEGIN llmcc local Codex provider.*?# END llmcc local Codex provider\s*'
if ($config -match $pattern) {
	$config = [regex]::Replace($config, $pattern, $managedBlock + [Environment]::NewLine)
} else {
	$config = $config.TrimEnd() + [Environment]::NewLine + [Environment]::NewLine + $managedBlock + [Environment]::NewLine
}

$utf8 = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($codexConfig, $config.TrimStart() , $utf8)

Write-Host "Configured Codex local provider in $codexConfig"
