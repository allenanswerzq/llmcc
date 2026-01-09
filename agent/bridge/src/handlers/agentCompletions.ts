// Agent-mode chat completions handler with built-in tool execution

import * as http from 'http';
import * as vscode from 'vscode';
import {
    ChatCompletionRequest,
    ChatCompletionResponse,
    ChatCompletionChunk,
    ToolCall,
    getModelSelector,
    generateRequestId
} from '../types';
import {
    executeTool,
    parseToolCalls,
    isToolAvailable,
    getAvailableTools,
    formatBuiltinToolsForPrompt,
    getBuiltinToolDocs,
    ToolDefinition as InternalToolDef
} from '@llmcc/tools';

// Configuration
const ENABLE_AGENT_MODE = true;
const MAX_AGENT_ITERATIONS = 5;
const ALWAYS_INJECT_BUILTIN_TOOLS = true; // Always add our tools even if client doesn't request them

export async function handleAgentChatCompletions(
    request: ChatCompletionRequest,
    res: http.ServerResponse
): Promise<void> {
    const requestId = generateRequestId();
    const created = Math.floor(Date.now() / 1000);

    // Always inject built-in tools if enabled
    if (ALWAYS_INJECT_BUILTIN_TOOLS) {
        request.tools = request.tools || [];
        const existingToolNames = new Set(request.tools.map(t => t.function.name));
        const builtinDocs = getBuiltinToolDocs();

        for (const doc of builtinDocs) {
            if (!existingToolNames.has(doc.name)) {
                request.tools.push({
                    type: 'function',
                    function: {
                        name: doc.name,
                        description: doc.description,
                        parameters: {
                            type: 'object',
                            properties: Object.fromEntries(
                                doc.parameters.map(p => [p.name, { type: p.type, description: p.description }])
                            ),
                            required: doc.parameters.filter(p => p.required).map(p => p.name)
                        }
                    }
                });
            }
        }
        console.log(`[Agent] Injected built-in tools. Total tools: ${request.tools.length}`);
    }

    // Check if we should use agent mode
    const hasTools = request.tools && request.tools.length > 0;
    const useAgentMode = ENABLE_AGENT_MODE && hasTools;

    if (!useAgentMode) {
        // Fall back to regular handler - import dynamically to avoid circular deps
        const { handleChatCompletions } = await import('./chatCompletions');
        return handleChatCompletions(request, res);
    }

    console.log(`[Agent] Starting agent mode with ${request.tools?.length} tools`);

    // Get the model
    const modelSelector = getModelSelector(request.model);
    const models = await vscode.lm.selectChatModels({
        vendor: modelSelector.vendor,
        family: modelSelector.family,
    });

    if (models.length === 0) {
        const fallbackModels = await vscode.lm.selectChatModels({ vendor: 'copilot' });
        if (fallbackModels.length === 0) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                error: { message: 'No language models available', type: 'service_unavailable' }
            }));
            return;
        }
        models.push(fallbackModels[0]);
    }

    const model = models[0];
    console.log(`[Agent] Using model: ${model.id}`);

    // Build tool definitions for the system prompt
    const toolDefs = buildToolDefinitions(request.tools!);
    console.log(`[Agent] Tool definitions: ${toolDefs.map(t => t.name).join(', ')}`);

    // Build system prompt with tool instructions
    const toolSystemPrompt = buildToolSystemPrompt(toolDefs);
    console.log(`[Agent] System prompt length: ${toolSystemPrompt.length} chars`);

    // Convert messages and prepend tool system prompt
    const vscodeMessages = buildMessagesWithToolPrompt(request.messages, toolSystemPrompt);
    console.log(`[Agent] Total messages: ${vscodeMessages.length}`);

    try {
        if (request.stream) {
            await handleAgentStreaming(model, vscodeMessages, toolDefs, res, requestId, created, request.model);
        } else {
            await handleAgentNonStreaming(model, vscodeMessages, toolDefs, res, requestId, created, request.model);
        }
    } catch (error) {
        console.error('[Agent] Error:', error);
        const message = error instanceof Error ? error.message : 'Unknown error';
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: { message, type: 'internal_error' } }));
    }
}

function buildToolDefinitions(tools: NonNullable<ChatCompletionRequest['tools']>): InternalToolDef[] {
    return tools.map(t => ({
        name: t.function.name,
        description: t.function.description || '',
        parameters: t.function.parameters || {}
    }));
}

function buildToolSystemPrompt(tools: InternalToolDef[]): string {
    const availableBuiltin = getAvailableTools();

    // Separate builtin tools from client-provided tools
    const builtinToolNames = tools.filter(t => availableBuiltin.includes(t.name)).map(t => t.name);
    const externalTools = tools.filter(t => !availableBuiltin.includes(t.name));

    // Build the prompt with detailed builtin tool documentation
    let prompt = `CRITICAL: You MUST use tools to complete tasks. DO NOT explain how to use tools - USE THEM DIRECTLY.

=== EXECUTABLE BUILT-IN TOOLS ===
These tools are executed automatically. Use them by outputting the JSON format shown.

${formatBuiltinToolsForPrompt()}
`;

    // Add external tools if any
    if (externalTools.length > 0) {
        const externalDescriptions = externalTools.map(t =>
            `- ${t.name}: ${t.description}`
        ).join('\n');
        prompt += `
=== EXTERNAL TOOLS (handled by client) ===
${externalDescriptions}
`;
    }

    prompt += `
=== HOW TO USE TOOLS ===
Output EXACTLY this JSON format (no markdown, no code blocks, no explanation):
{"tool": "tool_name", "arg1": "value1", "arg2": "value2"}

IMPORTANT:
- Output ONLY the raw JSON object
- Do NOT wrap in code blocks
- Do NOT add explanatory text before or after
- Wait for the tool result before continuing`;

    return prompt;
}

function buildMessagesWithToolPrompt(
    messages: ChatCompletionRequest['messages'],
    toolSystemPrompt: string
): vscode.LanguageModelChatMessage[] {
    const vscodeMessages: vscode.LanguageModelChatMessage[] = [];

    // Add tool system prompt first
    vscodeMessages.push(vscode.LanguageModelChatMessage.User(`[System Instructions]:\n${toolSystemPrompt}`));

    for (const msg of messages) {
        const content = msg.content ?? '';

        if (msg.role === 'user') {
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(content));
        } else if (msg.role === 'assistant') {
            if (msg.tool_calls?.length) {
                const toolCallsText = msg.tool_calls.map(tc =>
                    `{"tool": "${tc.function.name}", "arguments": ${tc.function.arguments}}`
                ).join('\n');
                vscodeMessages.push(vscode.LanguageModelChatMessage.Assistant(toolCallsText));
            } else {
                vscodeMessages.push(vscode.LanguageModelChatMessage.Assistant(content));
            }
        } else if (msg.role === 'tool') {
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(
                `[Tool Result]: ${content}`
            ));
        } else if (msg.role === 'system') {
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(`[System]: ${content}`));
        }
    }

    return vscodeMessages;
}

async function handleAgentNonStreaming(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    tools: InternalToolDef[],
    res: http.ServerResponse,
    requestId: string,
    created: number,
    requestedModel: string
): Promise<void> {
    let currentMessages = [...messages];
    let iteration = 0;
    let finalContent = '';
    const allToolCalls: ToolCall[] = [];

    while (iteration < MAX_AGENT_ITERATIONS) {
        iteration++;
        console.log(`[Agent] Iteration ${iteration}`);

        // Get model response
        const response = await model.sendRequest(currentMessages, {});
        let responseText = '';
        for await (const chunk of response.text) {
            responseText += chunk;
        }

        console.log(`[Agent] Response: ${responseText.substring(0, 200)}...`);

        // Check for tool calls
        const toolCalls = parseToolCalls(responseText);

        if (toolCalls.length === 0) {
            // No tool calls - we're done
            finalContent = responseText;
            break;
        }

        // Execute first tool call
        const call = toolCalls[0];
        console.log(`[Agent] Executing: ${call.name}`);

        if (!isToolAvailable(call.name)) {
            // Tool not available - return as tool_call for client to handle
            const toolCallObj: ToolCall = {
                id: `call_${Date.now()}`,
                type: 'function',
                function: {
                    name: call.name,
                    arguments: JSON.stringify(call.arguments)
                }
            };
            allToolCalls.push(toolCallObj);

            // Return response asking client to execute tool
            const completionResponse: ChatCompletionResponse = {
                id: requestId,
                object: 'chat.completion',
                created,
                model: requestedModel,
                choices: [{
                    index: 0,
                    message: {
                        role: 'assistant',
                        content: null,
                        tool_calls: [toolCallObj]
                    },
                    finish_reason: 'tool_calls',
                }],
                usage: { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 }
            };

            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify(completionResponse));
            return;
        }

        // Execute the tool
        const result = await executeTool(call.name, call.arguments);
        const resultText = result.success
            ? result.output
            : `Error: ${result.error}`;

        console.log(`[Agent] Tool result: ${resultText.substring(0, 100)}...`);

        // Add to conversation
        currentMessages.push(vscode.LanguageModelChatMessage.Assistant(responseText));
        currentMessages.push(vscode.LanguageModelChatMessage.User(`[Tool Result for ${call.name}]:\n${resultText}`));
    }

    // Build final response
    const completionResponse: ChatCompletionResponse = {
        id: requestId,
        object: 'chat.completion',
        created,
        model: requestedModel,
        choices: [{
            index: 0,
            message: {
                role: 'assistant',
                content: finalContent,
            },
            finish_reason: 'stop',
        }],
        usage: {
            prompt_tokens: estimateTokens(messages),
            completion_tokens: estimateTokens(finalContent),
            total_tokens: 0
        }
    };
    completionResponse.usage.total_tokens =
        completionResponse.usage.prompt_tokens + completionResponse.usage.completion_tokens;

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(completionResponse));
}

async function handleAgentStreaming(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    tools: InternalToolDef[],
    res: http.ServerResponse,
    requestId: string,
    created: number,
    requestedModel: string
): Promise<void> {
    res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
    });

    // Send initial chunk
    const initialChunk: ChatCompletionChunk = {
        id: requestId,
        object: 'chat.completion.chunk',
        created,
        model: requestedModel,
        choices: [{ index: 0, delta: { role: 'assistant' }, finish_reason: null }]
    };
    res.write(`data: ${JSON.stringify(initialChunk)}\n\n`);

    let currentMessages = [...messages];
    let iteration = 0;
    let hasMoreTools = true;

    while (hasMoreTools && iteration < MAX_AGENT_ITERATIONS) {
        iteration++;

        // Get model response
        const response = await model.sendRequest(currentMessages, {});
        let responseText = '';

        // Stream the response
        for await (const text of response.text) {
            responseText += text;
            const chunk: ChatCompletionChunk = {
                id: requestId,
                object: 'chat.completion.chunk',
                created,
                model: requestedModel,
                choices: [{ index: 0, delta: { content: text }, finish_reason: null }]
            };
            res.write(`data: ${JSON.stringify(chunk)}\n\n`);
        }

        // Check for tool calls
        const toolCalls = parseToolCalls(responseText);

        if (toolCalls.length === 0) {
            hasMoreTools = false;
            break;
        }

        // Execute tool
        const call = toolCalls[0];

        if (!isToolAvailable(call.name)) {
            // Stream a message about unavailable tool
            const msg = `\n[Tool ${call.name} not available locally - would need external execution]\n`;
            res.write(`data: ${JSON.stringify({
                id: requestId,
                object: 'chat.completion.chunk',
                created,
                model: requestedModel,
                choices: [{ index: 0, delta: { content: msg }, finish_reason: null }]
            })}\n\n`);
            hasMoreTools = false;
            break;
        }

        // Execute and stream result
        const result = await executeTool(call.name, call.arguments);
        const resultText = result.success ? result.output : `Error: ${result.error}`;

        // Stream tool execution info
        const toolInfo = `\n\n[Executing ${call.name}...]\n${resultText}\n\n`;
        res.write(`data: ${JSON.stringify({
            id: requestId,
            object: 'chat.completion.chunk',
            created,
            model: requestedModel,
            choices: [{ index: 0, delta: { content: toolInfo }, finish_reason: null }]
        })}\n\n`);

        // Add to conversation for next iteration
        currentMessages.push(vscode.LanguageModelChatMessage.Assistant(responseText));
        currentMessages.push(vscode.LanguageModelChatMessage.User(`[Tool Result for ${call.name}]:\n${resultText}`));
    }

    // Send final chunk
    const finalChunk: ChatCompletionChunk = {
        id: requestId,
        object: 'chat.completion.chunk',
        created,
        model: requestedModel,
        choices: [{ index: 0, delta: {}, finish_reason: 'stop' }]
    };
    res.write(`data: ${JSON.stringify(finalChunk)}\n\n`);
    res.write('data: [DONE]\n\n');
    res.end();
}

function estimateTokens(input: string | vscode.LanguageModelChatMessage[]): number {
    if (typeof input === 'string') {
        return Math.ceil(input.length / 4);
    }
    return input.reduce((acc, m) => {
        const content = typeof m.content === 'string' ? m.content : '';
        return acc + Math.ceil(content.length / 4);
    }, 0);
}
