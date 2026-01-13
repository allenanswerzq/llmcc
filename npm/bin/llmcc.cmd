@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
set "BINARY=%SCRIPT_DIR%llmcc-win32-x64.exe"

if exist "%BINARY%" (
    "%BINARY%" %*
    exit /b %errorlevel%
)

echo Error: No binary found for Windows x64 >&2
echo Please report this issue: https://github.com/allenanswerzq/llmcc/issues >&2
exit /b 1
