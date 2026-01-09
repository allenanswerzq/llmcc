import * as http from 'http';
import * as vscode from 'vscode';
import { getModelSelector, generateRequestId } from '../types';

// OpenAI Responses API types
interface ResponsesRequest {
    model: string;
    input: string | ResponseInput[];
    instructions?: string;
    stream?: boolean;
    temperature?: number;
    max_output_tokens?: number;
    tools?: Tool[];
    tool_choice?: string | { type: string; name?: string };
    previous_response_id?: string;
}

interface ResponseInput {
    type: 'message';
    role: 'user' | 'assistant' | 'system';
    content: string | ContentPart[];
}

interface ContentPart {
    type: 'input_text' | 'output_text' | 'text';
    text: string;
}

interface Tool {
    type: string;
    name?: string;
    description?: string;
    parameters?: Record<string, unknown>;
}

interface ResponseObject {
    id: string;
    object: 'response';
    created_at: number;
    status: 'completed' | 'in_progress' | 'failed';
    model: string;
    output: OutputItem[];
    usage?: {
        input_tokens: number;
        output_tokens: number;
        total_tokens: number;
    };
}

interface OutputItem {
    type: 'message';
    id: string;
    role: 'assistant';
    content: ContentPart[];
}

export async function handleResponses(
    request: ResponsesRequest,
    res: http.ServerResponse
): Promise<void> {
    const requestId = generateRequestId();
    const created = Math.floor(Date.now() / 1000);

    // Get the model selector
    const modelSelector = getModelSelector(request.model);

    console.log('[API Bridge] Responses API full request:', JSON.stringify(request, null, 2));
    console.log(`[API Bridge] Responses API request for model: ${request.model} -> ${modelSelector.vendor}/${modelSelector.family}`);

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

    // Convert input to VS Code messages
    const vscodeMessages = convertInputToMessages(request.input, request.instructions);

    try {
        if (request.stream) {
            await handleStreamingResponses(model, vscodeMessages, res, requestId, created, request.model);
        } else {
            await handleNonStreamingResponses(model, vscodeMessages, res, requestId, created, request.model);
        }
    } catch (error) {
        console.error('[API Bridge] Error during responses:', error);

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

function convertInputToMessages(
    input: string | ResponseInput[],
    instructions?: string
): vscode.LanguageModelChatMessage[] {
    const messages: vscode.LanguageModelChatMessage[] = [];

    // Add system instructions if provided
    if (instructions) {
        messages.push(vscode.LanguageModelChatMessage.User(`[System Instructions]: ${instructions}`));
    }

    if (typeof input === 'string') {
        // Simple string input
        messages.push(vscode.LanguageModelChatMessage.User(input));
    } else {
        // Array of message objects
        for (const msg of input) {
            const content = extractTextContent(msg.content);
            if (msg.role === 'user') {
                messages.push(vscode.LanguageModelChatMessage.User(content));
            } else if (msg.role === 'assistant') {
                messages.push(vscode.LanguageModelChatMessage.Assistant(content));
            } else if (msg.role === 'system') {
                messages.push(vscode.LanguageModelChatMessage.User(`[System]: ${content}`));
            }
        }
    }

    return messages;
}

function extractTextContent(content: string | ContentPart[]): string {
    if (typeof content === 'string') {
        return content;
    }
    return content
        .filter(part => part.type === 'input_text' || part.type === 'output_text' || part.type === 'text')
        .map(part => part.text)
        .join('');
}

async function handleNonStreamingResponses(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    res: http.ServerResponse,
    requestId: string,
    created: number,
    requestedModel: string
): Promise<void> {
    const response = await model.sendRequest(messages, {});

    let fullContent = '';
    for await (const chunk of response.text) {
        fullContent += chunk;
    }

    const responseObject: ResponseObject = {
        id: requestId,
        object: 'response',
        created_at: created,
        status: 'completed',
        model: requestedModel,
        output: [{
            type: 'message',
            id: `msg_${requestId}`,
            role: 'assistant',
            content: [{
                type: 'output_text',
                text: fullContent
            }]
        }],
        usage: {
            input_tokens: estimateTokens(messages.map(m => getMessageText(m)).join('')),
            output_tokens: estimateTokens(fullContent),
            total_tokens: 0
        }
    };

    responseObject.usage!.total_tokens =
        responseObject.usage!.input_tokens + responseObject.usage!.output_tokens;

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(responseObject));
}

async function handleStreamingResponses(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
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

    const response = await model.sendRequest(messages, {});

    // Send initial event
    const startEvent = {
        type: 'response.created',
        response: {
            id: requestId,
            object: 'response',
            created_at: created,
            status: 'in_progress',
            model: requestedModel,
            output: []
        }
    };
    res.write(`event: ${startEvent.type}\ndata: ${JSON.stringify(startEvent)}\n\n`);

    // Send content start
    const contentStartEvent = {
        type: 'response.output_item.added',
        output_index: 0,
        item: {
            type: 'message',
            id: `msg_${requestId}`,
            role: 'assistant',
            content: []
        }
    };
    res.write(`event: ${contentStartEvent.type}\ndata: ${JSON.stringify(contentStartEvent)}\n\n`);

    // Stream content
    let fullContent = '';
    for await (const text of response.text) {
        fullContent += text;
        const deltaEvent = {
            type: 'response.output_text.delta',
            output_index: 0,
            content_index: 0,
            delta: text
        };
        res.write(`event: ${deltaEvent.type}\ndata: ${JSON.stringify(deltaEvent)}\n\n`);
    }

    // Send completion
    const doneEvent = {
        type: 'response.output_text.done',
        output_index: 0,
        content_index: 0,
        text: fullContent
    };
    res.write(`event: ${doneEvent.type}\ndata: ${JSON.stringify(doneEvent)}\n\n`);

    // Send output_item.done - CRITICAL for Codex to display output
    const itemDoneEvent = {
        type: 'response.output_item.done',
        output_index: 0,
        item: {
            type: 'message',
            id: `msg_${requestId}`,
            role: 'assistant',
            content: [{
                type: 'output_text',
                text: fullContent
            }]
        }
    };
    res.write(`event: ${itemDoneEvent.type}\ndata: ${JSON.stringify(itemDoneEvent)}\n\n`);

    const completedEvent = {
        type: 'response.completed',
        response: {
            id: requestId,
            object: 'response',
            created_at: created,
            status: 'completed',
            model: requestedModel,
            output: [{
                type: 'message',
                id: `msg_${requestId}`,
                role: 'assistant',
                content: [{
                    type: 'output_text',
                    text: fullContent
                }]
            }],
            usage: {
                input_tokens: estimateTokens(messages.map(m => getMessageText(m)).join('')),
                output_tokens: estimateTokens(fullContent),
                total_tokens: estimateTokens(messages.map(m => getMessageText(m)).join('')) + estimateTokens(fullContent)
            }
        }
    };
    res.write(`event: ${completedEvent.type}\ndata: ${JSON.stringify(completedEvent)}\n\n`);
    res.write('data: [DONE]\n\n');
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
