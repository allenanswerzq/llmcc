import * as http from 'http';
import * as vscode from 'vscode';
import {
    ChatCompletionRequest,
    ChatCompletionResponse,
    ChatCompletionChunk,
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
    const vscodMessages = request.messages.map(msg => {
        if (msg.role === 'user') {
            return vscode.LanguageModelChatMessage.User(msg.content);
        } else if (msg.role === 'assistant') {
            return vscode.LanguageModelChatMessage.Assistant(msg.content);
        } else {
            // System messages - prepend to first user message or add as user
            return vscode.LanguageModelChatMessage.User(`[System]: ${msg.content}`);
        }
    });

    // Build request options
    const options: vscode.LanguageModelChatRequestOptions = {};

    // Note: VS Code LM API has limited options compared to OpenAI
    // temperature, max_tokens etc. are not directly supported

    try {
        if (request.stream) {
            await handleStreamingResponse(model, vscodMessages, options, res, requestId, created, request.model);
        } else {
            await handleNonStreamingResponse(model, vscodMessages, options, res, requestId, created, request.model);
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

    // Collect all chunks
    let fullContent = '';
    for await (const chunk of response.text) {
        fullContent += chunk;
    }

    const completionResponse: ChatCompletionResponse = {
        id: requestId,
        object: 'chat.completion',
        created,
        model: requestedModel,
        choices: [{
            index: 0,
            message: {
                role: 'assistant',
                content: fullContent,
            },
            finish_reason: 'stop',
        }],
        usage: {
            prompt_tokens: estimateTokens(getMessageContents(messages)),
            completion_tokens: estimateTokens(fullContent),
            total_tokens: 0, // Will be calculated
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

    // Stream content chunks
    for await (const text of response.text) {
        const chunk: ChatCompletionChunk = {
            id: requestId,
            object: 'chat.completion.chunk',
            created,
            model: requestedModel,
            choices: [{
                index: 0,
                delta: { content: text },
                finish_reason: null,
            }],
        };
        res.write(`data: ${JSON.stringify(chunk)}\n\n`);
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
            finish_reason: 'stop',
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
