# Test built-in tool execution

Write-Host "=== Testing Built-in Tool Execution ===" -ForegroundColor Cyan

# Test 1: bash tool
Write-Host "`nTest 1: Bash Tool" -ForegroundColor Yellow
$body = @{
    model = "gpt-4o"
    messages = @(
        @{ role = "system"; content = "You are a helpful assistant. When asked to run commands, output a JSON tool call like: {`"tool`": `"bash`", `"command`": `"your command`"}" }
        @{ role = "user"; content = "Run the command: echo hello world" }
    )
    tools = @(
        @{
            type = "function"
            function = @{
                name = "bash"
                description = "Execute a shell command"
                parameters = @{
                    type = "object"
                    properties = @{
                        command = @{ type = "string"; description = "The command to run" }
                    }
                    required = @("command")
                }
            }
        }
    )
    stream = $false
} | ConvertTo-Json -Depth 10

try {
    $response = Invoke-RestMethod -Uri "http://localhost:5168/v1/chat/completions" -Method Post -Body $body -ContentType "application/json"
    Write-Host "Response:" -ForegroundColor Green
    $response.choices[0].message.content
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# Test 2: read_file tool
Write-Host "`nTest 2: Read File Tool" -ForegroundColor Yellow
$body = @{
    model = "gpt-4o"
    messages = @(
        @{ role = "system"; content = "You are a helpful assistant. Use tools by outputting JSON: {`"tool`": `"read_file`", `"path`": `"/path/to/file`"}" }
        @{ role = "user"; content = "Read the file package.json in c:\Users\zhangqiang\llmcc\bridge" }
    )
    tools = @(
        @{
            type = "function"
            function = @{
                name = "read_file"
                description = "Read contents of a file"
                parameters = @{
                    type = "object"
                    properties = @{
                        path = @{ type = "string"; description = "Path to the file" }
                    }
                }
            }
        }
    )
    stream = $false
} | ConvertTo-Json -Depth 10

try {
    $response = Invoke-RestMethod -Uri "http://localhost:5168/v1/chat/completions" -Method Post -Body $body -ContentType "application/json"
    Write-Host "Response:" -ForegroundColor Green
    $response.choices[0].message.content
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# Test 3: list_dir tool
Write-Host "`nTest 3: List Directory Tool" -ForegroundColor Yellow
$body = @{
    model = "gpt-4o"
    messages = @(
        @{ role = "system"; content = "Use tools by outputting JSON. Example: {`"tool`": `"list_dir`", `"path`": `".`"}" }
        @{ role = "user"; content = "List the files in c:\Users\zhangqiang\llmcc\bridge" }
    )
    tools = @(
        @{
            type = "function"
            function = @{
                name = "list_dir"
                description = "List directory contents"
                parameters = @{
                    type = "object"
                    properties = @{
                        path = @{ type = "string"; description = "Directory path" }
                    }
                }
            }
        }
    )
    stream = $false
} | ConvertTo-Json -Depth 10

try {
    $response = Invoke-RestMethod -Uri "http://localhost:5168/v1/chat/completions" -Method Post -Body $body -ContentType "application/json"
    Write-Host "Response:" -ForegroundColor Green
    $response.choices[0].message.content
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host "`n=== Tests Complete ===" -ForegroundColor Cyan
