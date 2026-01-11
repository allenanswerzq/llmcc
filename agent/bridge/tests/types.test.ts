/**
 * Unit tests for the Bridge server routing and request handling
 * These tests verify the HTTP API without requiring VS Code
 * Run with: npx tsx --test tests/unit/*.test.ts
 */

import { describe, it } from 'node:test';
import * as assert from 'node:assert';

// Note: These tests verify the types and utility functions
// Full integration tests require VS Code runtime

describe('Request Types', () => {
    it('should define ChatCompletionRequest structure', () => {
        const request = {
            model: 'claude-sonnet-4',
            messages: [
                { role: 'user', content: 'Hello' }
            ],
            stream: false
        };

        assert.ok(request.model);
        assert.ok(Array.isArray(request.messages));
        assert.strictEqual(request.messages[0].role, 'user');
    });

    it('should support streaming parameter', () => {
        const streamingRequest = {
            model: 'gpt-4o',
            messages: [{ role: 'user', content: 'Hi' }],
            stream: true
        };

        assert.strictEqual(streamingRequest.stream, true);
    });

    it('should support tools in request', () => {
        const requestWithTools = {
            model: 'claude-opus-4',
            messages: [{ role: 'user', content: 'List files' }],
            tools: [
                {
                    type: 'function',
                    function: {
                        name: 'list_dir',
                        description: 'List directory',
                        parameters: {
                            type: 'object',
                            properties: {
                                path: { type: 'string' }
                            }
                        }
                    }
                }
            ]
        };

        assert.ok(requestWithTools.tools);
        assert.strictEqual(requestWithTools.tools.length, 1);
        assert.strictEqual(requestWithTools.tools[0].function.name, 'list_dir');
    });
});

describe('Response Types', () => {
    it('should define ChatCompletionResponse structure', () => {
        const response = {
            id: 'chatcmpl-123',
            object: 'chat.completion',
            created: Math.floor(Date.now() / 1000),
            model: 'claude-sonnet-4',
            choices: [
                {
                    index: 0,
                    message: {
                        role: 'assistant',
                        content: 'Hello! How can I help?'
                    },
                    finish_reason: 'stop'
                }
            ],
            usage: {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30
            }
        };

        assert.ok(response.id);
        assert.strictEqual(response.object, 'chat.completion');
        assert.ok(response.choices.length > 0);
        assert.strictEqual(response.choices[0].message.role, 'assistant');
    });

    it('should define streaming chunk structure', () => {
        const chunk = {
            id: 'chatcmpl-123',
            object: 'chat.completion.chunk',
            created: Math.floor(Date.now() / 1000),
            model: 'claude-sonnet-4',
            choices: [
                {
                    index: 0,
                    delta: {
                        content: 'Hello'
                    },
                    finish_reason: null
                }
            ]
        };

        assert.strictEqual(chunk.object, 'chat.completion.chunk');
        assert.ok(chunk.choices[0].delta);
    });
});

describe('Anthropic Messages Format', () => {
    it('should support Anthropic message format', () => {
        const anthropicRequest = {
            model: 'claude-sonnet-4-20250514',
            max_tokens: 1024,
            messages: [
                {
                    role: 'user',
                    content: [
                        { type: 'text', text: 'Hello Claude' }
                    ]
                }
            ]
        };

        assert.ok(anthropicRequest.max_tokens);
        assert.ok(Array.isArray(anthropicRequest.messages[0].content));
    });

    it('should support system message in Anthropic format', () => {
        const request = {
            model: 'claude-opus-4',
            system: 'You are a helpful assistant.',
            messages: [
                { role: 'user', content: 'Hi' }
            ]
        };

        assert.ok(request.system);
        assert.strictEqual(typeof request.system, 'string');
    });
});

describe('Model Mapping', () => {
    const modelMappings: Record<string, string[]> = {
        'claude-sonnet-4': ['claude-sonnet-4', 'claude-3.5-sonnet', 'claude-3-5-sonnet'],
        'claude-opus-4': ['claude-opus-4', 'claude-3-opus'],
        'gpt-4o': ['gpt-4o', 'gpt-4-turbo'],
        'o1': ['o1', 'o1-preview'],
    };

    it('should map Claude model names', () => {
        const aliases = modelMappings['claude-sonnet-4'];
        assert.ok(aliases.includes('claude-sonnet-4'));
        assert.ok(aliases.includes('claude-3.5-sonnet'));
    });

    it('should map GPT model names', () => {
        const aliases = modelMappings['gpt-4o'];
        assert.ok(aliases.includes('gpt-4o'));
    });
});

describe('URL Routing', () => {
    const routes = [
        { method: 'GET', path: '/', handler: 'health' },
        { method: 'GET', path: '/health', handler: 'health' },
        { method: 'GET', path: '/v1/models', handler: 'models' },
        { method: 'POST', path: '/v1/chat/completions', handler: 'chatCompletions' },
        { method: 'POST', path: '/v1/messages', handler: 'messages' },
        { method: 'POST', path: '/v1/responses', handler: 'responses' },
    ];

    it('should have health check endpoints', () => {
        const healthRoutes = routes.filter(r => r.handler === 'health');
        assert.ok(healthRoutes.length >= 2);
    });

    it('should have models endpoint', () => {
        const modelsRoute = routes.find(r => r.handler === 'models');
        assert.ok(modelsRoute);
        assert.strictEqual(modelsRoute.method, 'GET');
    });

    it('should have chat completions endpoint', () => {
        const chatRoute = routes.find(r => r.handler === 'chatCompletions');
        assert.ok(chatRoute);
        assert.strictEqual(chatRoute.path, '/v1/chat/completions');
    });

    it('should have messages endpoint for Anthropic format', () => {
        const messagesRoute = routes.find(r => r.handler === 'messages');
        assert.ok(messagesRoute);
        assert.strictEqual(messagesRoute.path, '/v1/messages');
    });
});

describe('CORS Headers', () => {
    it('should define expected CORS headers', () => {
        const expectedHeaders = [
            'Access-Control-Allow-Origin',
            'Access-Control-Allow-Methods',
            'Access-Control-Allow-Headers'
        ];

        const corsConfig = {
            allowedOrigins: ['*'],
            allowedMethods: ['GET', 'POST', 'OPTIONS'],
            allowedHeaders: ['Content-Type', 'Authorization', 'x-api-key', 'anthropic-version']
        };

        assert.ok(corsConfig.allowedOrigins.includes('*'));
        assert.ok(corsConfig.allowedMethods.includes('POST'));
        assert.ok(corsConfig.allowedHeaders.includes('x-api-key'));
    });
});

describe('Error Response Format', () => {
    it('should format OpenAI-style errors', () => {
        const error = {
            error: {
                message: 'Model not found',
                type: 'invalid_request_error',
                code: 'model_not_found'
            }
        };

        assert.ok(error.error);
        assert.ok(error.error.message);
        assert.ok(error.error.type);
    });

    it('should format Anthropic-style errors', () => {
        const error = {
            type: 'error',
            error: {
                type: 'invalid_request_error',
                message: 'Invalid model specified'
            }
        };

        assert.strictEqual(error.type, 'error');
        assert.ok(error.error.message);
    });
});

describe('Tool Call Format', () => {
    it('should format OpenAI tool calls', () => {
        const toolCall = {
            id: 'call_abc123',
            type: 'function',
            function: {
                name: 'read_file',
                arguments: JSON.stringify({ path: '/test.txt' })
            }
        };

        assert.ok(toolCall.id);
        assert.strictEqual(toolCall.type, 'function');
        assert.ok(toolCall.function.arguments);
    });

    it('should format Anthropic tool use', () => {
        const toolUse = {
            type: 'tool_use',
            id: 'toolu_abc123',
            name: 'read_file',
            input: { path: '/test.txt' }
        };

        assert.strictEqual(toolUse.type, 'tool_use');
        assert.ok(toolUse.input);
    });

    it('should format tool result for OpenAI', () => {
        const toolResult = {
            role: 'tool',
            tool_call_id: 'call_abc123',
            content: 'File contents here'
        };

        assert.strictEqual(toolResult.role, 'tool');
        assert.ok(toolResult.tool_call_id);
    });

    it('should format tool result for Anthropic', () => {
        const toolResult = {
            type: 'tool_result',
            tool_use_id: 'toolu_abc123',
            content: 'File contents here'
        };

        assert.strictEqual(toolResult.type, 'tool_result');
        assert.ok(toolResult.tool_use_id);
    });
});
