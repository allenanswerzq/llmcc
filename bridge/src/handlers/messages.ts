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
}

interface AnthropicMessage {
    role: 'user' | 'assistant';
    content: string | ContentBlock[];
}

interface ContentBlock {
    type: 'text' | 'image';
    text?: string;
}

interface MessagesResponse {
    id: string;
    type: 'message';
    role: 'assistant';
    content: ContentBlock[];
    model: string;
    stop_reason: 'end_turn' | 'max_tokens' | 'stop_sequence' | null;
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

    try {
        if (request.stream) {
            await handleStreamingMessages(model, vscodeMessages, res, requestId, request.model);
        } else {
            await handleNonStreamingMessages(model, vscodeMessages, res, requestId, request.model);
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
        const content = extractContent(msg.content);
        if (msg.role === 'user') {
            vscodeMessages.push(vscode.LanguageModelChatMessage.User(content));
        } else if (msg.role === 'assistant') {
            vscodeMessages.push(vscode.LanguageModelChatMessage.Assistant(content));
        }
    }

    return vscodeMessages;
}

function extractContent(content: string | ContentBlock[]): string {
    if (typeof content === 'string') {
        return content;
    }
    return content
        .filter(block => block.type === 'text' && block.text)
        .map(block => block.text!)
        .join('');
}

async function handleNonStreamingMessages(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    res: http.ServerResponse,
    requestId: string,
    requestedModel: string
): Promise<void> {
    const response = await model.sendRequest(messages, {});

    let fullContent = '';
    for await (const chunk of response.text) {
        fullContent += chunk;
    }

    const messagesResponse: MessagesResponse = {
        id: `msg_${requestId}`,
        type: 'message',
        role: 'assistant',
        content: [{
            type: 'text',
            text: fullContent
        }],
        model: requestedModel,
        stop_reason: 'end_turn',
        stop_sequence: null,
        usage: {
            input_tokens: estimateTokens(messages.map(m => getMessageText(m)).join('')),
            output_tokens: estimateTokens(fullContent)
        }
    };

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(messagesResponse));
}

async function handleStreamingMessages(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    res: http.ServerResponse,
    requestId: string,
    requestedModel: string
): Promise<void> {
    res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
    });

    const response = await model.sendRequest(messages, {});

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

    // Send content_block_start
    const contentBlockStart = {
        type: 'content_block_start',
        index: 0,
        content_block: {
            type: 'text',
            text: ''
        }
    };
    res.write(`event: content_block_start\ndata: ${JSON.stringify(contentBlockStart)}\n\n`);

    // Stream content deltas
    let fullContent = '';
    for await (const text of response.text) {
        fullContent += text;
        const delta = {
            type: 'content_block_delta',
            index: 0,
            delta: {
                type: 'text_delta',
                text: text
            }
        };
        res.write(`event: content_block_delta\ndata: ${JSON.stringify(delta)}\n\n`);
    }

    // Send content_block_stop
    const contentBlockStop = {
        type: 'content_block_stop',
        index: 0
    };
    res.write(`event: content_block_stop\ndata: ${JSON.stringify(contentBlockStop)}\n\n`);

    // Send message_delta with stop reason
    const messageDelta = {
        type: 'message_delta',
        delta: {
            stop_reason: 'end_turn',
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
