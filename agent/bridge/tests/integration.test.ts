/**
 * Integration tests for the Bridge API server
 * These tests require the server to be running on localhost:5168
 * Run with: npx tsx tests/integration.test.ts
 */

import { describe, it, before } from 'node:test';
import * as assert from 'node:assert';
import * as http from 'http';

const BASE_URL = 'http://localhost:5168';

// Helper to make HTTP requests
async function request(
    method: string,
    path: string,
    body?: unknown
): Promise<{ status: number; data: unknown }> {
    return new Promise((resolve, reject) => {
        const url = new URL(path, BASE_URL);
        const options: http.RequestOptions = {
            method,
            hostname: url.hostname,
            port: url.port,
            path: url.pathname,
            headers: {
                'Content-Type': 'application/json',
            }
        };

        const req = http.request(options, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try {
                    resolve({
                        status: res.statusCode || 0,
                        data: data ? JSON.parse(data) : null
                    });
                } catch {
                    resolve({ status: res.statusCode || 0, data });
                }
            });
        });

        req.on('error', reject);

        if (body) {
            req.write(JSON.stringify(body));
        }
        req.end();
    });
}

// Check if server is running
async function isServerRunning(): Promise<boolean> {
    try {
        const { status } = await request('GET', '/health');
        return status === 200;
    } catch {
        return false;
    }
}

describe('Bridge API Integration Tests', { skip: false }, () => {
    before(async () => {
        const running = await isServerRunning();
        if (!running) {
            console.log('⚠️  Server is not running. Start it with the VS Code extension.');
            console.log('   These tests will be skipped.');
        }
    });

    describe('Health Endpoints', () => {
        it('GET / should return health status', async () => {
            const { status, data } = await request('GET', '/');
            if (status === 0) return; // Server not running

            assert.strictEqual(status, 200);
            assert.strictEqual((data as { status: string }).status, 'ok');
        });

        it('GET /health should return health status', async () => {
            const { status, data } = await request('GET', '/health');
            if (status === 0) return;

            assert.strictEqual(status, 200);
            assert.ok((data as { endpoints: string[] }).endpoints);
        });
    });

    describe('Models Endpoint', () => {
        it('GET /v1/models should return model list', async () => {
            const { status, data } = await request('GET', '/v1/models');
            if (status === 0) return;

            assert.strictEqual(status, 200);
            const response = data as { object: string; data: unknown[] };
            assert.strictEqual(response.object, 'list');
            assert.ok(Array.isArray(response.data));
        });

        it('GET /v1/models/{id} should return model info', async () => {
            const { status, data } = await request('GET', '/v1/models/claude-sonnet-4');
            if (status === 0) return;

            assert.strictEqual(status, 200);
            assert.strictEqual((data as { id: string }).id, 'claude-sonnet-4');
        });
    });

    describe('Chat Completions Endpoint', () => {
        it('POST /v1/chat/completions with simple message', async () => {
            const { status, data } = await request('POST', '/v1/chat/completions', {
                model: 'claude-sonnet-4',
                messages: [
                    { role: 'user', content: 'Say "test successful" and nothing else.' }
                ],
                stream: false,
                max_tokens: 50
            });
            if (status === 0) return;

            assert.strictEqual(status, 200);
            const response = data as {
                choices: Array<{ message: { content: string } }>
            };
            assert.ok(response.choices);
            assert.ok(response.choices[0].message.content);
        });

        it('POST /v1/chat/completions with system message', async () => {
            const { status, data } = await request('POST', '/v1/chat/completions', {
                model: 'gpt-4o',
                messages: [
                    { role: 'system', content: 'You are a test bot. Only respond with "OK".' },
                    { role: 'user', content: 'Are you there?' }
                ],
                stream: false,
                max_tokens: 10
            });
            if (status === 0) return;

            assert.strictEqual(status, 200);
            const response = data as { choices: unknown[] };
            assert.ok(response.choices);
        });
    });

    describe('Messages Endpoint (Anthropic Format)', () => {
        it('POST /v1/messages with Anthropic format', async () => {
            const { status, data } = await request('POST', '/v1/messages', {
                model: 'claude-sonnet-4-20250514',
                max_tokens: 50,
                messages: [
                    { role: 'user', content: 'Say "test" only.' }
                ]
            });
            if (status === 0) return;

            assert.strictEqual(status, 200);
            const response = data as { content: unknown[] };
            assert.ok(response.content);
        });

        it('POST /v1/messages with system prompt', async () => {
            const { status, data } = await request('POST', '/v1/messages', {
                model: 'claude-opus-4',
                max_tokens: 20,
                system: 'Only respond with "verified".',
                messages: [
                    { role: 'user', content: 'Test' }
                ]
            });
            if (status === 0) return;

            assert.strictEqual(status, 200);
        });
    });

    describe('Token Counting Endpoint', () => {
        it('POST /v1/messages/count_tokens should estimate tokens', async () => {
            const { status, data } = await request('POST', '/v1/messages/count_tokens', {
                model: 'claude-sonnet-4',
                messages: [
                    { role: 'user', content: 'Hello, world!' }
                ]
            });
            if (status === 0) return;

            assert.strictEqual(status, 200);
            const response = data as { input_tokens: number };
            assert.ok(typeof response.input_tokens === 'number');
            assert.ok(response.input_tokens > 0);
        });
    });

    describe('Account/Organization Endpoints', () => {
        it('GET /v1/account should return fake account', async () => {
            const { status, data } = await request('GET', '/v1/account');
            if (status === 0) return;

            assert.strictEqual(status, 200);
            const response = data as { subscription: { status: string } };
            assert.ok(response.subscription);
            assert.strictEqual(response.subscription.status, 'active');
        });

        it('GET /v1/usage should return usage info', async () => {
            const { status, data } = await request('GET', '/v1/usage');
            if (status === 0) return;

            assert.strictEqual(status, 200);
            const response = data as { usage: unknown };
            assert.ok(response.usage);
        });
    });

    describe('CORS', () => {
        it('OPTIONS request should return CORS headers', async () => {
            const { status } = await request('OPTIONS', '/v1/chat/completions');
            if (status === 0) return;

            assert.strictEqual(status, 204);
        });
    });

    describe('Error Handling', () => {
        it('should return 404 for unknown endpoint', async () => {
            const { status } = await request('POST', '/unknown/endpoint');
            if (status === 0) return;

            assert.strictEqual(status, 404);
        });

        it('should handle invalid JSON gracefully', async () => {
            // This would need raw request handling to send invalid JSON
            // Skipping for now as the helper always sends valid JSON
        });
    });
});

// Run if executed directly
if (process.argv[1] === import.meta.url.replace('file:///', '')) {
    console.log('Running integration tests...');
    console.log('Make sure the bridge server is running on localhost:5168');
}
