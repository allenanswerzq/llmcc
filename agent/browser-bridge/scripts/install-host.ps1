# Install Native Messaging host for Browser Bridge (Windows)
# Run this in PowerShell as Administrator if you want system-wide installation

param(
    [switch]$SystemWide = $false
)

$ErrorActionPreference = "Stop"

# Get paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BridgeDir = Split-Path -Parent $ScriptDir
$HostPath = Join-Path $BridgeDir "dist\index.js"
$WrapperPath = Join-Path $BridgeDir "browser-bridge-host.bat"

# Host name (must match what Claude Code looks for)
$HostName = "com.anthropic.browser_extension"

# Create wrapper batch file
@"
@echo off
cd /d "$BridgeDir"
node "$HostPath" %*
"@ | Out-File -FilePath $WrapperPath -Encoding ASCII

Write-Host "Created wrapper: $WrapperPath"

# Create manifest
$Manifest = @{
    name = $HostName
    description = "Browser Bridge for Claude Code"
    path = $WrapperPath
    type = "stdio"
    allowed_origins = @(
        "chrome-extension://jmclflgclhepglnfbelejpdmelliocij/"
    )
}

$ManifestJson = $Manifest | ConvertTo-Json -Depth 3

# Determine installation paths
if ($SystemWide) {
    # System-wide installation (requires admin)
    $ChromePaths = @(
        "HKLM:\SOFTWARE\Google\Chrome\NativeMessagingHosts\$HostName",
        "HKLM:\SOFTWARE\Microsoft\Edge\NativeMessagingHosts\$HostName",
        "HKLM:\SOFTWARE\BraveSoftware\Brave-Browser\NativeMessagingHosts\$HostName"
    )
} else {
    # User installation
    $ChromePaths = @(
        "HKCU:\SOFTWARE\Google\Chrome\NativeMessagingHosts\$HostName",
        "HKCU:\SOFTWARE\Microsoft\Edge\NativeMessagingHosts\$HostName",
        "HKCU:\SOFTWARE\BraveSoftware\Brave-Browser\NativeMessagingHosts\$HostName"
    )
}

# Create manifest file
$ManifestDir = Join-Path $BridgeDir "manifest"
$ManifestPath = Join-Path $ManifestDir "$HostName.json"

if (!(Test-Path $ManifestDir)) {
    New-Item -ItemType Directory -Path $ManifestDir -Force | Out-Null
}

$ManifestJson | Out-File -FilePath $ManifestPath -Encoding UTF8
Write-Host "Created manifest: $ManifestPath"

# Register with browsers
foreach ($RegPath in $ChromePaths) {
    try {
        # Create registry key path if it doesn't exist
        $ParentPath = Split-Path -Parent $RegPath
        if (!(Test-Path $ParentPath)) {
            New-Item -Path $ParentPath -Force | Out-Null
        }

        # Set the registry key
        if (!(Test-Path $RegPath)) {
            New-Item -Path $RegPath -Force | Out-Null
        }
        Set-ItemProperty -Path $RegPath -Name "(Default)" -Value $ManifestPath
        Write-Host "Registered: $RegPath"
    } catch {
        Write-Host "Skipped: $RegPath (might need admin privileges)"
    }
}

Write-Host ""
Write-Host "Installation complete!"
Write-Host ""
Write-Host "To test the installation:"
Write-Host "  1. Build the project: npm run build"
Write-Host "  2. Test: claude --chrome -p 'Navigate to example.com'"
