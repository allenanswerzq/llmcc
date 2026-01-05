# Test script for Tool/Function Calling in Copilot API Bridge

param(
    [string]$BaseUrl = "http://localhost:5168"
)

Write-Host "=== Testing Tool/Function Calling ===" -ForegroundColor Cyan
Write-Host "Base URL: $BaseUrl`n"

# Helper function to make API calls
function Invoke-ApiTest {
    param(
        [string]$Name,
        [string]$Endpoint,
        [hashtable]$Body
    )

    Write-Host "$Name" -ForegroundColor Yellow
    Write-Host ("-" * 50)

    try {
        $json = $Body | ConvertTo-Json -Depth 10
        Write-Host "Request:" -ForegroundColor Gray
        Write-Host $json -ForegroundColor DarkGray
        Write-Host ""

        $response = Invoke-RestMethod -Uri "$BaseUrl$Endpoint" -Method Post -Body $json -ContentType "application/json"
        Write-Host "Response:" -ForegroundColor Green
        $response | ConvertTo-Json -Depth 10
        return $response
    } catch {
        Write-Host "Error: $_" -ForegroundColor Red
        return $null
    }
}

# ============================================
# Test 1: OpenAI format with tools (non-streaming)
# ============================================
Write-Host "`n"
$test1 = Invoke-ApiTest -Name "Test 1: OpenAI Chat Completion with Tools" -Endpoint "/v1/chat/completions" -Body @{
    model = "gpt-4o"
    messages = @(
        @{
            role = "user"
            content = "What's the weather in San Francisco? Use the get_weather tool."
        }
    )
    tools = @(
        @{
            type = "function"
            function = @{
                name = "get_weather"
                description = "Get the current weather for a location"
                parameters = @{
                    type = "object"
                    properties = @{
                        location = @{
                            type = "string"
                            description = "The city and state, e.g. San Francisco, CA"
                        }
                        unit = @{
                            type = "string"
                            enum = @("celsius", "fahrenheit")
                            description = "Temperature unit"
                        }
                    }
                    required = @("location")
                }
            }
        }
    )
    tool_choice = "auto"
    stream = $false
}

if ($test1 -and $test1.choices[0].message.tool_calls) {
    Write-Host "`nTool calls detected!" -ForegroundColor Green
    $test1.choices[0].message.tool_calls | ForEach-Object {
        Write-Host "  Tool: $($_.function.name)" -ForegroundColor Cyan
        Write-Host "  Args: $($_.function.arguments)" -ForegroundColor Cyan
    }
} else {
    Write-Host "`nNo tool calls in response (model may have responded with text)" -ForegroundColor Yellow
}

# ============================================
# Test 2: OpenAI format - Tool result continuation
# ============================================
Write-Host "`n"
$test2 = Invoke-ApiTest -Name "Test 2: OpenAI Chat with Tool Result" -Endpoint "/v1/chat/completions" -Body @{
    model = "gpt-4o"
    messages = @(
        @{
            role = "user"
            content = "What's the weather in San Francisco?"
        },
        @{
            role = "assistant"
            content = $null
            tool_calls = @(
                @{
                    id = "call_abc123"
                    type = "function"
                    function = @{
                        name = "get_weather"
                        arguments = '{"location": "San Francisco, CA"}'
                    }
                }
            )
        },
        @{
            role = "tool"
            tool_call_id = "call_abc123"
            content = '{"temperature": 72, "condition": "sunny", "humidity": 45}'
        }
    )
    stream = $false
}

# ============================================
# Test 3: Anthropic Messages API with tools
# ============================================
Write-Host "`n"
$test3 = Invoke-ApiTest -Name "Test 3: Anthropic Messages API with Tools" -Endpoint "/v1/messages" -Body @{
    model = "claude-sonnet-4"
    max_tokens = 1024
    messages = @(
        @{
            role = "user"
            content = "Read the file /tmp/example.txt using the read_file tool"
        }
    )
    tools = @(
        @{
            name = "read_file"
            description = "Read contents of a file from the filesystem"
            input_schema = @{
                type = "object"
                properties = @{
                    path = @{
                        type = "string"
                        description = "The absolute path to the file"
                    }
                }
                required = @("path")
            }
        },
        @{
            name = "write_file"
            description = "Write content to a file"
            input_schema = @{
                type = "object"
                properties = @{
                    path = @{
                        type = "string"
                        description = "The absolute path to the file"
                    }
                    content = @{
                        type = "string"
                        description = "The content to write"
                    }
                }
                required = @("path", "content")
            }
        }
    )
    stream = $false
}

if ($test3 -and $test3.stop_reason -eq "tool_use") {
    Write-Host "`nTool use detected!" -ForegroundColor Green
    $test3.content | Where-Object { $_.type -eq "tool_use" } | ForEach-Object {
        Write-Host "  Tool: $($_.name)" -ForegroundColor Cyan
        Write-Host "  ID: $($_.id)" -ForegroundColor Cyan
        Write-Host "  Input: $($_.input | ConvertTo-Json -Compress)" -ForegroundColor Cyan
    }
} else {
    Write-Host "`nNo tool_use in response (model may have responded with text)" -ForegroundColor Yellow
}

# ============================================
# Test 4: Anthropic Messages API - Tool result continuation
# ============================================
Write-Host "`n"
$test4 = Invoke-ApiTest -Name "Test 4: Anthropic Messages with Tool Result" -Endpoint "/v1/messages" -Body @{
    model = "claude-sonnet-4"
    max_tokens = 1024
    messages = @(
        @{
            role = "user"
            content = "Read the file /tmp/example.txt"
        },
        @{
            role = "assistant"
            content = @(
                @{
                    type = "text"
                    text = "I'll read that file for you."
                },
                @{
                    type = "tool_use"
                    id = "toolu_abc123"
                    name = "read_file"
                    input = @{
                        path = "/tmp/example.txt"
                    }
                }
            )
        },
        @{
            role = "user"
            content = @(
                @{
                    type = "tool_result"
                    tool_use_id = "toolu_abc123"
                    content = "Hello, this is the content of the file!"
                }
            )
        }
    )
    stream = $false
}

# ============================================
# Test 5: Multiple tools
# ============================================
Write-Host "`n"
$test5 = Invoke-ApiTest -Name "Test 5: Multiple Tools Available" -Endpoint "/v1/chat/completions" -Body @{
    model = "claude-opus-4.5"
    messages = @(
        @{
            role = "user"
            content = "Run the command 'ls -la' to list files in the current directory"
        }
    )
    tools = @(
        @{
            type = "function"
            function = @{
                name = "bash"
                description = "Execute a bash command"
                parameters = @{
                    type = "object"
                    properties = @{
                        command = @{
                            type = "string"
                            description = "The bash command to execute"
                        }
                    }
                    required = @("command")
                }
            }
        },
        @{
            type = "function"
            function = @{
                name = "read_file"
                description = "Read a file from disk"
                parameters = @{
                    type = "object"
                    properties = @{
                        path = @{
                            type = "string"
                            description = "File path to read"
                        }
                    }
                    required = @("path")
                }
            }
        },
        @{
            type = "function"
            function = @{
                name = "write_file"
                description = "Write content to a file"
                parameters = @{
                    type = "object"
                    properties = @{
                        path = @{
                            type = "string"
                            description = "File path to write"
                        }
                        content = @{
                            type = "string"
                            description = "Content to write"
                        }
                    }
                    required = @("path", "content")
                }
            }
        }
    )
    stream = $false
}

# ============================================
# Summary
# ============================================
Write-Host "`n"
Write-Host "=== Test Summary ===" -ForegroundColor Cyan
Write-Host "Test 1 (OpenAI tools): $(if($test1){'PASSED'}else{'FAILED'})" -ForegroundColor $(if($test1){'Green'}else{'Red'})
Write-Host "Test 2 (OpenAI tool result): $(if($test2){'PASSED'}else{'FAILED'})" -ForegroundColor $(if($test2){'Green'}else{'Red'})
Write-Host "Test 3 (Anthropic tools): $(if($test3){'PASSED'}else{'FAILED'})" -ForegroundColor $(if($test3){'Green'}else{'Red'})
Write-Host "Test 4 (Anthropic tool result): $(if($test4){'PASSED'}else{'FAILED'})" -ForegroundColor $(if($test4){'Green'}else{'Red'})
Write-Host "Test 5 (Multiple tools): $(if($test5){'PASSED'}else{'FAILED'})" -ForegroundColor $(if($test5){'Green'}else{'Red'})

Write-Host "`nNote: 'PASSED' means the API responded without error." -ForegroundColor Gray
Write-Host "Tool invocation depends on whether the model decides to use tools." -ForegroundColor Gray
Write-Host "`n=== Tests Complete ===" -ForegroundColor Cyan
