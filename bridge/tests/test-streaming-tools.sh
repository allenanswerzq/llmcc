#!/bin/bash
# Test streaming tool calls for Copilot API Bridge

BASE_URL="${1:-http://localhost:5168}"

echo "=== Testing Streaming Tool Calls ==="
echo "Base URL: $BASE_URL"
echo ""

# Test 1: OpenAI streaming with tools
echo "Test 1: OpenAI Streaming with Tools"
echo "----------------------------------------"
curl -N -X POST "${BASE_URL}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Get the weather in Tokyo using the get_weather function"
            }
        ],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather for a location",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"]
                    }
                }
            }
        ],
        "stream": true
    }'

echo ""
echo ""

# Test 2: Anthropic streaming with tools
echo "Test 2: Anthropic Streaming with Tools"
echo "----------------------------------------"
curl -N -X POST "${BASE_URL}/v1/messages" \
    -H "Content-Type: application/json" \
    -d '{
        "model": "claude-sonnet-4",
        "max_tokens": 1024,
        "messages": [
            {
                "role": "user",
                "content": "Use the bash tool to run: echo hello"
            }
        ],
        "tools": [
            {
                "name": "bash",
                "description": "Run a bash command",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"}
                    },
                    "required": ["command"]
                }
            }
        ],
        "stream": true
    }'

echo ""
echo ""
echo "=== Streaming Tests Complete ==="
