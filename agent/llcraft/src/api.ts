/**
 * API client for llcraft
 * Supports both Anthropic API and OpenAI-compatible APIs (like Copilot bridge)
 * With tool calling support including browser tools
 */

import { Message, Config, APIMessage } from './types.js';
import { builtinTools, executeTool, ToolCall, ToolDefinition, llmccTool } from './tools.js';
import { browserTools, isBrowserTool, executeBrowserTool } from './browser.js';

interface OpenAIMessage {
    role: 'system' | 'user' | 'assistant' | 'tool';
    content: string | null;
    tool_calls?: ToolCall[];
    tool_call_id?: string;
}

interface OpenAIRequest {
    model: string;
    messages: OpenAIMessage[];
    max_tokens?: number;
    tools?: ToolDefinition[];
    tool_choice?: 'auto' | 'none';
}

interface OpenAIChoice {
    message: {
        content: string | null;
        tool_calls?: ToolCall[];
    };
    finish_reason: string;
}

interface OpenAIResponse {
    choices: OpenAIChoice[];
}

interface AnthropicResponse {
    content: Array<{
        type: 'text';
        text: string;
    }>;
}

/**
 * Call API - auto-detects Anthropic vs OpenAI format based on URL
 * Handles tool calling with automatic execution loop
 */
export async function callAPI(
    messages: Message[],
    config: Config,
    onToolCall?: (name: string, args: string, thinking?: string) => void
): Promise<string> {
    // Convert messages to API format
    const apiMessages: APIMessage[] = messages
        .filter(m => m.role === 'user' || m.role === 'assistant')
        .map(m => ({
            role: m.role as 'user' | 'assistant',
            content: m.content,
        }));

    // Use OpenAI format for Copilot bridge (localhost:5168)
    const useCopilotFormat = config.baseUrl.includes('localhost:5168') ||
                              config.baseUrl.includes('127.0.0.1:5168');

    if (useCopilotFormat) {
        return callOpenAIAPIWithTools(apiMessages, config, onToolCall);
    } else {
        return callAnthropicAPI(apiMessages, config);
    }
}

/**
 * OpenAI-compatible API call with tool support
 * Uses structured output format for tools since Copilot bridge doesn't support native tool calling
 */
async function callOpenAIAPIWithTools(
    messages: APIMessage[],
    config: Config,
    onToolCall?: (name: string, args: string, thinking?: string) => void
): Promise<string> {
    // Collect available tools
    const availableTools = [...builtinTools];
    if (config.llmcc) {
        availableTools.push(llmccTool);
    }
    if (config.chrome) {
        availableTools.push(...browserTools);
    }

    // Build system prompt with tool instructions
    const toolDescriptions = availableTools.map(t => {
        const f = t.function;
        const params = JSON.stringify(f.parameters.properties, null, 2);
        return `- ${f.name}: ${f.description}\n  Parameters: ${params}`;
    }).join('\n\n');

    const toolSystemPrompt = `${config.systemPrompt}

You have access to the following tools. To use a tool, output a JSON block in this EXACT format:
\`\`\`tool
{"name": "tool_name", "arguments": {"param1": "value1"}}
\`\`\`

Available tools:
${toolDescriptions}

After receiving tool results, provide your final response to the user.
Only use tools when necessary to complete the user's request.`;

    let openaiMessages: OpenAIMessage[] = [
        { role: 'system', content: toolSystemPrompt },
        ...messages.map(m => ({
            role: m.role as 'user' | 'assistant',
            content: m.content,
        })),
    ];

    const maxIterations = 50;  // Increased for complex tasks
    let iterations = 0;

    while (iterations < maxIterations) {
        iterations++;

        const request: OpenAIRequest = {
            model: config.model,
            messages: openaiMessages,
            max_tokens: config.maxTokens,
        };

        const response = await fetch(`${config.baseUrl}/v1/chat/completions`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': `Bearer ${config.apiKey}`,
            },
            body: JSON.stringify(request),
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`API error ${response.status}: ${errorText}`);
        }

        const data = await response.json() as OpenAIResponse;
        const content = data.choices[0]?.message?.content || '';

        // Check for tool calls in the response
        const toolCallMatch = content.match(/```tool\s*\n?([\s\S]*?)\n?```/);

        if (toolCallMatch) {
            try {
                const toolRequest = JSON.parse(toolCallMatch[1]);
                const toolName = toolRequest.name;
                const toolArgs = toolRequest.arguments || {};

                // Extract thinking text (text before the tool block)
                const thinkingText = content.slice(0, content.indexOf('```tool')).trim();

                if (onToolCall) {
                    onToolCall(toolName, JSON.stringify(toolArgs), thinkingText);
                }

                // Create a synthetic tool call
                const syntheticToolCall: ToolCall = {
                    id: `call_${Date.now()}`,
                    type: 'function',
                    function: {
                        name: toolName,
                        arguments: JSON.stringify(toolArgs),
                    },
                };

                // Execute the appropriate tool
                let result;
                if (config.chrome && isBrowserTool(toolName)) {
                    result = await executeBrowserTool(syntheticToolCall);
                } else {
                    result = executeTool(syntheticToolCall);
                }

                // Add assistant message and tool result
                openaiMessages.push({
                    role: 'assistant',
                    content: content,
                });
                openaiMessages.push({
                    role: 'user',
                    content: `Tool result for ${toolName}:\n${result.content}`,
                });

                // Continue the loop to get the final response
                continue;
            } catch (e) {
                // Invalid JSON in tool block, return as-is
                return content;
            }
        }

        // No tool calls, return the content
        return content;
    }

    return '(max tool iterations reached)';
}

/**
 * Anthropic API call
 */
async function callAnthropicAPI(messages: APIMessage[], config: Config): Promise<string> {
    const request = {
        model: config.model,
        max_tokens: config.maxTokens,
        system: config.systemPrompt,
        messages: messages,
    };

    const response = await fetch(`${config.baseUrl}/v1/messages`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            'x-api-key': config.apiKey,
            'anthropic-version': '2023-06-01',
        },
        body: JSON.stringify(request),
    });

    if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`API error ${response.status}: ${errorText}`);
    }

    const data = await response.json() as AnthropicResponse;
    return data.content
        .filter(block => block.type === 'text')
        .map(block => block.text)
        .join('\n');
}
