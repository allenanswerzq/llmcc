import * as http from 'http';
import * as vscode from 'vscode';
import { getModelSelector, generateRequestId } from '../types';

// Anthropic Messages API types
interface MessagesRequest {
    model: string;
    messages: AnthropicMessage[];
    system?: string;
    max_tokens?: number;
    temperature?: number;
    stream?: boolean;
    stop_sequences?: string[];
    tools?: AnthropicTool[];
    tool_choice?: { type: 'auto' | 'any' | 'tool'; name?: string };
}

interface AnthropicTool {
    name: string;
    description?: string;
    input_schema: Record<string, unknown>;
}

interface AnthropicMessage {
    role: 'user' | 'assistant';
    content: string | ContentBlock[];
}

interface ContentBlock {
    type: 'text' | 'image' | 'tool_use' | 'tool_result';
    text?: string;
    id?: string;
    name?: string;
    input?: unknown;
    tool_use_id?: string;
    content?: string | ContentBlock[];
    is_error?: boolean;
}

interface MessagesResponse {
    id: string;
    type: 'message';
    role: 'assistant';
    content: ContentBlock[];
    model: string;
    stop_reason: 'end_turn' | 'max_tokens' | 'stop_sequence' | 'tool_use' | null;
    stop_sequence: string | null;
    usage: {
        input_tokens: number;
        output_tokens: number;
    };
}

export async function handleMessages(
    request: MessagesRequest,
    res: http.ServerResponse
): Promise<void> {
    const requestId = generateRequestId();

    // Get the model selector
    const modelSelector = getModelSelector(request.model);

    console.log(`[API Bridge] Messages API request for model: ${request.model} -> ${modelSelector.vendor}/${modelSelector.family}`);
    if (request.tools?.length) {
        console.log(`[API Bridge] Tools provided: ${request.tools.map(t => t.name).join(', ')}`);
    }

    // Select a chat model
    const models = await vscode.lm.selectChatModels({
        vendor: modelSelector.vendor,
        family: modelSelector.family,
    });

    if (models.length === 0) {
        const fallbackModels = await vscode.lm.selectChatModels({
            vendor: 'copilot',
        });

        if (fallbackModels.length === 0) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                type: 'error',
                error: {
                    type: 'service_unavailable',
                    message: 'No language models available. Make sure GitHub Copilot is active.'
                }
            }));
            return;
        }

        models.push(fallbackModels[0]);
    }

    const model = models[0];
    console.log(`[API Bridge] Using model: ${model.id} (${model.name})`);

    // Convert messages to VS Code format
    const vscodeMessages = convertMessagesToVSCode(request.messages, request.system);

    // Build request options with tools
    const options: vscode.LanguageModelChatRequestOptions = {};

    // Convert Anthropic tools to VS Code tools
    if (request.tools?.length) {
        options.tools = request.tools.map(tool => ({
            name: tool.name,
            description: tool.description || '',
            inputSchema: tool.input_schema
        }));
        console.log(`[API Bridge] Converted ${options.tools.length} tools to VS Code format`);
        console.log(`[API Bridge] Tools: ${options.tools.map(t => t.name).join(', ')}`);

        // Set tool mode based on tool_choice
        if (request.tool_choice?.type === 'any' || request.tool_choice?.type === 'tool') {
            options.toolMode = vscode.LanguageModelChatToolMode.Required;
        } else {
            options.toolMode = vscode.LanguageModelChatToolMode.Auto;
        }
    }

    try {
        if (request.stream) {
            await handleStreamingMessages(model, vscodeMessages, options, res, requestId, request.model);
        } else {
            await handleNonStreamingMessages(model, vscodeMessages, options, res, requestId, request.model);
        }
    } catch (error) {
        console.error('[API Bridge] Error during messages:', error);

        if (error instanceof vscode.LanguageModelError) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                type: 'error',
                error: {
                    type: 'invalid_request_error',
                    message: error.message
                }
            }));
        } else {
            const message = error instanceof Error ? error.message : 'Unknown error';
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                type: 'error',
                error: {
                    type: 'api_error',
                    message
                }
            }));
        }
    }
}

function convertMessagesToVSCode(
    messages: AnthropicMessage[],
    system?: string
): vscode.LanguageModelChatMessage[] {
    const vscodeMessages: vscode.LanguageModelChatMessage[] = [];

    // Add system message if provided
    if (system) {
        vscodeMessages.push(vscode.LanguageModelChatMessage.User(`[System]: ${system}`));
    }

    for (const msg of messages) {
        if (msg.role === 'user') {
            const content = extractContent(msg.content);
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(content));
        } else if (msg.role === 'assistant') {
            const content = extractContent(msg.content);
            vscodeMessages.push(vscode.LanguageModelChatMessage.Assistant(content));
        }
    }

    return vscodeMessages;
}

function extractContent(content: string | ContentBlock[]): string {
    if (typeof content === 'string') {
        return content;
    }

    const parts: string[] = [];

    for (const block of content) {
        if (block.type === 'text' && block.text) {
            parts.push(block.text);
        } else if (block.type === 'tool_use' && block.name) {
            // Include tool use in assistant message
            parts.push(`[Tool Call: ${block.name}(${JSON.stringify(block.input)})]`);
        } else if (block.type === 'tool_result' && block.tool_use_id) {
            // Include tool result
            const resultContent = typeof block.content === 'string'
                ? block.content
                : block.content?.map(b => b.type === 'text' ? b.text : '').join('') || '';
            parts.push(`[Tool Result for ${block.tool_use_id}]: ${resultContent}`);
        }
    }

    return parts.join('\n');
}

// Generate a unique tool use ID
function generateToolUseId(): string {
    return 'toolu_' + Math.random().toString(36).substring(2, 15);
}

async function handleNonStreamingMessages(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    options: vscode.LanguageModelChatRequestOptions,
    res: http.ServerResponse,
    requestId: string,
    requestedModel: string
): Promise<void> {
    const response = await model.sendRequest(messages, options);

    const contentBlocks: ContentBlock[] = [];
    let fullText = '';

    for await (const part of response.stream) {
        if (part instanceof vscode.LanguageModelTextPart) {
            fullText += part.value;
        } else if (part instanceof vscode.LanguageModelToolCallPart) {
            console.log(`[API Bridge] Tool use: ${part.name}(${JSON.stringify(part.input)})`);
            contentBlocks.push({
                type: 'tool_use',
                id: part.callId || generateToolUseId(),
                name: part.name,
                input: part.input
            });
        }
    }

    // Add text block if we have any text content
    if (fullText) {
        contentBlocks.unshift({
            type: 'text',
            text: fullText
        });
    }

    // If no content at all, add empty text block
    if (contentBlocks.length === 0) {
        contentBlocks.push({
            type: 'text',
            text: ''
        });
    }

    const hasToolUse = contentBlocks.some(b => b.type === 'tool_use');

    const messagesResponse: MessagesResponse = {
        id: `msg_${requestId}`,
        type: 'message',
        role: 'assistant',
        content: contentBlocks,
        model: requestedModel,
        stop_reason: hasToolUse ? 'tool_use' : 'end_turn',
        stop_sequence: null,
        usage: {
            input_tokens: estimateTokens(messages.map(m => getMessageText(m)).join('')),
            output_tokens: estimateTokens(fullText)
        }
    };

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(messagesResponse));
}

async function handleStreamingMessages(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    options: vscode.LanguageModelChatRequestOptions,
    res: http.ServerResponse,
    requestId: string,
    requestedModel: string
): Promise<void> {
    res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
    });

    const response = await model.sendRequest(messages, options);

    // Send message_start event
    const messageStart = {
        type: 'message_start',
        message: {
            id: `msg_${requestId}`,
            type: 'message',
            role: 'assistant',
            content: [],
            model: requestedModel,
            stop_reason: null,
            stop_sequence: null,
            usage: {
                input_tokens: estimateTokens(messages.map(m => getMessageText(m)).join('')),
                output_tokens: 0
            }
        }
    };
    res.write(`event: message_start\ndata: ${JSON.stringify(messageStart)}\n\n`);

    let contentBlockIndex = 0;
    let hasToolUse = false;
    let inTextBlock = false;
    let fullContent = '';

    for await (const part of response.stream) {
        if (part instanceof vscode.LanguageModelTextPart) {
            // Start text block if not already in one
            if (!inTextBlock) {
                const contentBlockStart = {
                    type: 'content_block_start',
                    index: contentBlockIndex,
                    content_block: {
                        type: 'text',
                        text: ''
                    }
                };
                res.write(`event: content_block_start\ndata: ${JSON.stringify(contentBlockStart)}\n\n`);
                inTextBlock = true;
            }

            fullContent += part.value;
            const delta = {
                type: 'content_block_delta',
                index: contentBlockIndex,
                delta: {
                    type: 'text_delta',
                    text: part.value
                }
            };
            res.write(`event: content_block_delta\ndata: ${JSON.stringify(delta)}\n\n`);
        } else if (part instanceof vscode.LanguageModelToolCallPart) {
            hasToolUse = true;
            console.log(`[API Bridge] Streaming tool use: ${part.name}`);

            // Close text block if open
            if (inTextBlock) {
                const contentBlockStop = {
                    type: 'content_block_stop',
                    index: contentBlockIndex
                };
                res.write(`event: content_block_stop\ndata: ${JSON.stringify(contentBlockStop)}\n\n`);
                contentBlockIndex++;
                inTextBlock = false;
            }

            // Send tool_use block
            const toolUseId = part.callId || generateToolUseId();
            const toolBlockStart = {
                type: 'content_block_start',
                index: contentBlockIndex,
                content_block: {
                    type: 'tool_use',
                    id: toolUseId,
                    name: part.name,
                    input: {}
                }
            };
            res.write(`event: content_block_start\ndata: ${JSON.stringify(toolBlockStart)}\n\n`);

            // Send input as delta
            const inputDelta = {
                type: 'content_block_delta',
                index: contentBlockIndex,
                delta: {
                    type: 'input_json_delta',
                    partial_json: typeof part.input === 'string' ? part.input : JSON.stringify(part.input)
                }
            };
            res.write(`event: content_block_delta\ndata: ${JSON.stringify(inputDelta)}\n\n`);

            // Close tool block
            const toolBlockStop = {
                type: 'content_block_stop',
                index: contentBlockIndex
            };
            res.write(`event: content_block_stop\ndata: ${JSON.stringify(toolBlockStop)}\n\n`);
            contentBlockIndex++;
        }
    }

    // Close text block if still open
    if (inTextBlock) {
        const contentBlockStop = {
            type: 'content_block_stop',
            index: contentBlockIndex
        };
        res.write(`event: content_block_stop\ndata: ${JSON.stringify(contentBlockStop)}\n\n`);
    }

    // Send message_delta with stop reason
    const messageDelta = {
        type: 'message_delta',
        delta: {
            stop_reason: hasToolUse ? 'tool_use' : 'end_turn',
            stop_sequence: null
        },
        usage: {
            output_tokens: estimateTokens(fullContent)
        }
    };
    res.write(`event: message_delta\ndata: ${JSON.stringify(messageDelta)}\n\n`);

    // Send message_stop
    const messageStop = {
        type: 'message_stop'
    };
    res.write(`event: message_stop\ndata: ${JSON.stringify(messageStop)}\n\n`);

    res.end();
}

function getMessageText(message: vscode.LanguageModelChatMessage): string {
    if (typeof message.content === 'string') {
        return message.content;
    }
    if (Array.isArray(message.content)) {
        return message.content
            .map(part => {
                if (typeof part === 'string') {
                    return part;
                }
                if ('value' in part) {
                    return String(part.value);
                }
                if ('text' in part) {
                    return String((part as { text: string }).text);
                }
                return '';
            })
            .join('');
    }
    return '';
}

function estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
}
