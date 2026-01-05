import * as http from 'http';
import * as vscode from 'vscode';
import {
    ChatCompletionRequest,
    ChatCompletionResponse,
    ChatCompletionChunk,
    ToolDefinition,
    ToolCall,
    ToolCallDelta,
    getModelSelector,
    generateRequestId
} from '../types';

export async function handleChatCompletions(
    request: ChatCompletionRequest,
    res: http.ServerResponse
): Promise<void> {
    const requestId = generateRequestId();
    const created = Math.floor(Date.now() / 1000);

    // Get the model selector
    const modelSelector = getModelSelector(request.model);

    console.log(`[API Bridge] Request for model: ${request.model} -> ${modelSelector.vendor}/${modelSelector.family}`);
    if (request.tools?.length) {
        console.log(`[API Bridge] Tools provided: ${request.tools.map(t => t.function.name).join(', ')}`);
    }

    // Select a chat model
    const models = await vscode.lm.selectChatModels({
        vendor: modelSelector.vendor,
        family: modelSelector.family,
    });

    if (models.length === 0) {
        // Try without family constraint
        const fallbackModels = await vscode.lm.selectChatModels({
            vendor: 'copilot',
        });

        if (fallbackModels.length === 0) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                error: {
                    message: 'No language models available. Make sure GitHub Copilot is active.',
                    type: 'service_unavailable',
                    code: 'model_not_available'
                }
            }));
            return;
        }

        models.push(fallbackModels[0]);
    }

    const model = models[0];
    console.log(`[API Bridge] Using model: ${model.id} (${model.name})`);

    // Convert messages to VS Code format
    const vscodeMessages = convertMessagesToVSCode(request.messages);

    // Build request options with tools
    const options: vscode.LanguageModelChatRequestOptions = {};

    // Convert OpenAI tools to VS Code tools
    if (request.tools?.length) {
        options.tools = convertToolsToVSCode(request.tools);
        console.log(`[API Bridge] Converted ${options.tools.length} tools to VS Code format`);
        console.log(`[API Bridge] Tools: ${options.tools.map(t => t.name).join(', ')}`);

        // Set tool mode based on tool_choice
        if (request.tool_choice === 'required') {
            options.toolMode = vscode.LanguageModelChatToolMode.Required;
        } else if (request.tool_choice === 'auto' || request.tool_choice === undefined) {
            // Auto mode - let the model decide
            options.toolMode = vscode.LanguageModelChatToolMode.Auto;
        }
        // 'none' means don't pass tools at all
        if (request.tool_choice === 'none') {
            delete options.tools;
        }
    }

    try {
        if (request.stream) {
            await handleStreamingResponse(model, vscodeMessages, options, res, requestId, created, request.model);
        } else {
            await handleNonStreamingResponse(model, vscodeMessages, options, res, requestId, created, request.model);
        }
    } catch (error) {
        console.error('[API Bridge] Error during chat completion:', error);

        if (error instanceof vscode.LanguageModelError) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                error: {
                    message: error.message,
                    type: 'language_model_error',
                    code: error.code
                }
            }));
        } else {
            const message = error instanceof Error ? error.message : 'Unknown error';
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                error: {
                    message,
                    type: 'internal_error'
                }
            }));
        }
    }
}

// Convert OpenAI messages to VS Code format
function convertMessagesToVSCode(messages: ChatCompletionRequest['messages']): vscode.LanguageModelChatMessage[] {
    const vscodeMessages: vscode.LanguageModelChatMessage[] = [];

    for (const msg of messages) {
        const content = msg.content ?? '';

        if (msg.role === 'user') {
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(content));
        } else if (msg.role === 'assistant') {
            if (msg.tool_calls?.length) {
                // Assistant message with tool calls - include tool call info
                const toolCallsText = msg.tool_calls.map(tc =>
                    `[Tool Call: ${tc.function.name}(${tc.function.arguments})]`
                ).join('\n');
                const fullContent = content ? `${content}\n${toolCallsText}` : toolCallsText;
                vscodeMessages.push(vscode.LanguageModelChatMessage.Assistant(fullContent));
            } else {
                vscodeMessages.push(vscode.LanguageModelChatMessage.Assistant(content));
            }
        } else if (msg.role === 'tool') {
            // Tool result - add as user message with context
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(
                `[Tool Result for ${msg.tool_call_id}]: ${content}`
            ));
        } else if (msg.role === 'system') {
            // System messages - prepend to first user message or add as user
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(`[System]: ${content}`));
        }
    }

    return vscodeMessages;
}

// Convert OpenAI tools to VS Code tools
function convertToolsToVSCode(tools: ToolDefinition[]): vscode.LanguageModelChatTool[] {
    return tools.map(tool => ({
        name: tool.function.name,
        description: tool.function.description || '',
        inputSchema: tool.function.parameters
    }));
}

// Generate a unique tool call ID
function generateToolCallId(): string {
    return 'call_' + Math.random().toString(36).substring(2, 15);
}

async function handleNonStreamingResponse(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    options: vscode.LanguageModelChatRequestOptions,
    res: http.ServerResponse,
    requestId: string,
    created: number,
    requestedModel: string
): Promise<void> {
    const response = await model.sendRequest(messages, options);

    // Collect all chunks and tool calls
    let fullContent = '';
    const toolCalls: ToolCall[] = [];

    for await (const part of response.stream) {
        if (part instanceof vscode.LanguageModelTextPart) {
            fullContent += part.value;
        } else if (part instanceof vscode.LanguageModelToolCallPart) {
            console.log(`[API Bridge] Tool call: ${part.name}(${JSON.stringify(part.input)})`);
            toolCalls.push({
                id: part.callId || generateToolCallId(),
                type: 'function',
                function: {
                    name: part.name,
                    arguments: typeof part.input === 'string' ? part.input : JSON.stringify(part.input)
                }
            });
        }
    }

    const hasToolCalls = toolCalls.length > 0;

    const completionResponse: ChatCompletionResponse = {
        id: requestId,
        object: 'chat.completion',
        created,
        model: requestedModel,
        choices: [{
            index: 0,
            message: {
                role: 'assistant',
                content: hasToolCalls ? null : fullContent,
                ...(hasToolCalls && { tool_calls: toolCalls })
            },
            finish_reason: hasToolCalls ? 'tool_calls' : 'stop',
        }],
        usage: {
            prompt_tokens: estimateTokens(getMessageContents(messages)),
            completion_tokens: estimateTokens(fullContent),
            total_tokens: 0,
        },
    };

    completionResponse.usage.total_tokens =
        completionResponse.usage.prompt_tokens + completionResponse.usage.completion_tokens;

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(completionResponse));
}

async function handleStreamingResponse(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    options: vscode.LanguageModelChatRequestOptions,
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

    const response = await model.sendRequest(messages, options);

    // Send initial chunk with role
    const initialChunk: ChatCompletionChunk = {
        id: requestId,
        object: 'chat.completion.chunk',
        created,
        model: requestedModel,
        choices: [{
            index: 0,
            delta: { role: 'assistant' },
            finish_reason: null,
        }],
    };
    res.write(`data: ${JSON.stringify(initialChunk)}\n\n`);

    // Track tool calls for finish reason
    let hasToolCalls = false;
    let toolCallIndex = 0;

    // Stream content chunks
    for await (const part of response.stream) {
        if (part instanceof vscode.LanguageModelTextPart) {
            const chunk: ChatCompletionChunk = {
                id: requestId,
                object: 'chat.completion.chunk',
                created,
                model: requestedModel,
                choices: [{
                    index: 0,
                    delta: { content: part.value },
                    finish_reason: null,
                }],
            };
            res.write(`data: ${JSON.stringify(chunk)}\n\n`);
        } else if (part instanceof vscode.LanguageModelToolCallPart) {
            hasToolCalls = true;
            console.log(`[API Bridge] Streaming tool call: ${part.name}`);

            // Send tool call in chunks
            const toolCallDelta: ToolCallDelta = {
                index: toolCallIndex,
                id: part.callId || generateToolCallId(),
                type: 'function',
                function: {
                    name: part.name,
                    arguments: typeof part.input === 'string' ? part.input : JSON.stringify(part.input)
                }
            };

            const chunk: ChatCompletionChunk = {
                id: requestId,
                object: 'chat.completion.chunk',
                created,
                model: requestedModel,
                choices: [{
                    index: 0,
                    delta: { tool_calls: [toolCallDelta] },
                    finish_reason: null,
                }],
            };
            res.write(`data: ${JSON.stringify(chunk)}\n\n`);
            toolCallIndex++;
        }
    }

    // Send final chunk
    const finalChunk: ChatCompletionChunk = {
        id: requestId,
        object: 'chat.completion.chunk',
        created,
        model: requestedModel,
        choices: [{
            index: 0,
            delta: {},
            finish_reason: hasToolCalls ? 'tool_calls' : 'stop',
        }],
    };
    res.write(`data: ${JSON.stringify(finalChunk)}\n\n`);
    res.write('data: [DONE]\n\n');
    res.end();
}

// Extract text content from messages
function getMessageContents(messages: vscode.LanguageModelChatMessage[]): string {
    return messages.map(m => {
        if (typeof m.content === 'string') {
            return m.content;
        }
        // content is an array of parts
        return (m.content as Array<{ value?: string }>)
            .filter(part => part && typeof part.value === 'string')
            .map(part => part.value)
            .join('');
    }).join('');
}

// Simple token estimation (rough approximation)
function estimateTokens(text: string): number {
    // Rough estimate: ~4 characters per token for English
    return Math.ceil(text.length / 4);
}
