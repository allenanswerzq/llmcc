#!/bin/bash
# Test script for Tool/Function Calling in Copilot API Bridge

BASE_URL="${1:-http://localhost:5168}"

echo "=== Testing Tool/Function Calling ==="
echo "Base URL: $BASE_URL"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
GRAY='\033[0;90m'
NC='\033[0m' # No Color

# Helper function
api_test() {
    local name="$1"
    local endpoint="$2"
    local body="$3"

    echo -e "${YELLOW}${name}${NC}"
    echo "----------------------------------------"
    echo -e "${GRAY}Request:${NC}"
    echo "$body" | jq . 2>/dev/null || echo "$body"
    echo ""

    response=$(curl -s -X POST "${BASE_URL}${endpoint}" \
        -H "Content-Type: application/json" \
        -d "$body")

    if [ $? -eq 0 ] && [ -n "$response" ]; then
        echo -e "${GREEN}Response:${NC}"
        echo "$response" | jq . 2>/dev/null || echo "$response"
        echo "$response"
        return 0
    else
        echo -e "${RED}Error: Request failed${NC}"
        return 1
    fi
}

# ============================================
# Test 1: OpenAI format with tools
# ============================================
echo ""
RESULT1=$(api_test "Test 1: OpenAI Chat Completion with Tools" "/v1/chat/completions" '{
    "model": "gpt-4o",
    "messages": [
        {
            "role": "user",
            "content": "What is the weather in San Francisco? Use the get_weather tool."
        }
    ],
    "tools": [
        {
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get the current weather for a location",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "The city and state, e.g. San Francisco, CA"
                        },
                        "unit": {
                            "type": "string",
                            "enum": ["celsius", "fahrenheit"],
                            "description": "Temperature unit"
                        }
                    },
                    "required": ["location"]
                }
            }
        }
    ],
    "tool_choice": "auto",
    "stream": false
}')
TEST1=$?

# Check for tool calls
if echo "$RESULT1" | jq -e '.choices[0].message.tool_calls' > /dev/null 2>&1; then
    echo -e "${GREEN}Tool calls detected!${NC}"
    echo "$RESULT1" | jq '.choices[0].message.tool_calls[] | "  Tool: \(.function.name), Args: \(.function.arguments)"' -r
fi

# ============================================
# Test 2: OpenAI format - Tool result
# ============================================
echo ""
RESULT2=$(api_test "Test 2: OpenAI Chat with Tool Result" "/v1/chat/completions" '{
    "model": "gpt-4o",
    "messages": [
        {
            "role": "user",
            "content": "What is the weather in San Francisco?"
        },
        {
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {
                    "id": "call_abc123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"location\": \"San Francisco, CA\"}"
                    }
                }
            ]
        },
        {
            "role": "tool",
            "tool_call_id": "call_abc123",
            "content": "{\"temperature\": 72, \"condition\": \"sunny\", \"humidity\": 45}"
        }
    ],
    "stream": false
}')
TEST2=$?

# ============================================
# Test 3: Anthropic Messages API with tools
# ============================================
echo ""
RESULT3=$(api_test "Test 3: Anthropic Messages API with Tools" "/v1/messages" '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [
        {
            "role": "user",
            "content": "Read the file /tmp/example.txt using the read_file tool"
        }
    ],
    "tools": [
        {
            "name": "read_file",
            "description": "Read contents of a file from the filesystem",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The absolute path to the file"
                    }
                },
                "required": ["path"]
            }
        }
    ],
    "stream": false
}')
TEST3=$?

# Check for tool_use
if echo "$RESULT3" | jq -e '.stop_reason == "tool_use"' > /dev/null 2>&1; then
    echo -e "${GREEN}Tool use detected!${NC}"
    echo "$RESULT3" | jq '.content[] | select(.type == "tool_use") | "  Tool: \(.name), ID: \(.id)"' -r
fi

# ============================================
# Test 4: Anthropic Messages API - Tool result
# ============================================
echo ""
RESULT4=$(api_test "Test 4: Anthropic Messages with Tool Result" "/v1/messages" '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [
        {
            "role": "user",
            "content": "Read the file /tmp/example.txt"
        },
        {
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "I will read that file for you."
                },
                {
                    "type": "tool_use",
                    "id": "toolu_abc123",
                    "name": "read_file",
                    "input": {
                        "path": "/tmp/example.txt"
                    }
                }
            ]
        },
        {
            "role": "user",
            "content": [
                {
                    "type": "tool_result",
                    "tool_use_id": "toolu_abc123",
                    "content": "Hello, this is the content of the file!"
                }
            ]
        }
    ],
    "stream": false
}')
TEST4=$?

# ============================================
# Test 5: Bash command tool
# ============================================
echo ""
RESULT5=$(api_test "Test 5: Bash Command Tool" "/v1/chat/completions" '{
    "model": "claude-opus-4.5",
    "messages": [
        {
            "role": "user",
            "content": "Run ls -la to list files"
        }
    ],
    "tools": [
        {
            "type": "function",
            "function": {
                "name": "bash",
                "description": "Execute a bash command",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The bash command to execute"
                        }
                    },
                    "required": ["command"]
                }
            }
        }
    ],
    "stream": false
}')
TEST5=$?

# ============================================
# Summary
# ============================================
echo ""
echo -e "${CYAN}=== Test Summary ===${NC}"
[ $TEST1 -eq 0 ] && echo -e "Test 1 (OpenAI tools): ${GREEN}PASSED${NC}" || echo -e "Test 1 (OpenAI tools): ${RED}FAILED${NC}"
[ $TEST2 -eq 0 ] && echo -e "Test 2 (OpenAI tool result): ${GREEN}PASSED${NC}" || echo -e "Test 2 (OpenAI tool result): ${RED}FAILED${NC}"
[ $TEST3 -eq 0 ] && echo -e "Test 3 (Anthropic tools): ${GREEN}PASSED${NC}" || echo -e "Test 3 (Anthropic tools): ${RED}FAILED${NC}"
[ $TEST4 -eq 0 ] && echo -e "Test 4 (Anthropic tool result): ${GREEN}PASSED${NC}" || echo -e "Test 4 (Anthropic tool result): ${RED}FAILED${NC}"
[ $TEST5 -eq 0 ] && echo -e "Test 5 (Bash tool): ${GREEN}PASSED${NC}" || echo -e "Test 5 (Bash tool): ${RED}FAILED${NC}"

echo ""
echo -e "${GRAY}Note: 'PASSED' means the API responded without error.${NC}"
echo -e "${GRAY}Tool invocation depends on whether the model decides to use tools.${NC}"
echo -e "${CYAN}=== Tests Complete ===${NC}"
