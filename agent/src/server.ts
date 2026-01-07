import * as http from 'http';
import * as vscode from 'vscode';
import { handleChatCompletions } from './handlers/chatCompletions';
import { handleAgentChatCompletions } from './handlers/agentCompletions';
import { handleModels } from './handlers/models';
import { handleResponses } from './handlers/responses';
import { handleMessages } from './handlers/messages';

export class ApiServer {
    private server: http.Server | null = null;
    private port: number;
    private bindAddress: string;
    private running = false;

    constructor(port: number, bindAddress: string = '0.0.0.0') {
        this.port = port;
        this.bindAddress = bindAddress;
    }

    async start(): Promise<void> {
        return new Promise((resolve, reject) => {
            this.server = http.createServer(async (req, res) => {
                await this.handleRequest(req, res);
            });

            this.server.on('error', (error: NodeJS.ErrnoException) => {
                if (error.code === 'EADDRINUSE') {
                    reject(new Error(`Port ${this.port} is already in use`));
                } else {
                    reject(error);
                }
            });

            this.server.listen(this.port, this.bindAddress, () => {
                this.running = true;
                console.log(`API Bridge server listening on http://${this.bindAddress}:${this.port}`);
                resolve();
            });
        });
    }

    async stop(): Promise<void> {
        return new Promise((resolve, reject) => {
            if (!this.server) {
                resolve();
                return;
            }

            this.server.close((err) => {
                if (err) {
                    reject(err);
                } else {
                    this.running = false;
                    this.server = null;
                    resolve();
                }
            });
        });
    }

    isRunning(): boolean {
        return this.running;
    }

    private async handleRequest(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
        // Set CORS headers
        const config = vscode.workspace.getConfiguration('copilot-api-bridge');
        const allowedOrigins = config.get<string[]>('allowedOrigins', ['*']);
        const origin = req.headers.origin || '*';

        if (allowedOrigins.includes('*') || allowedOrigins.includes(origin)) {
            res.setHeader('Access-Control-Allow-Origin', origin);
        }
        res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
        res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization, x-api-key, anthropic-version, anthropic-beta');

        // Handle preflight
        if (req.method === 'OPTIONS') {
            res.writeHead(204);
            res.end();
            return;
        }

        const url = req.url || '/';
        // Log all requests with headers for debugging
        console.log(`[API Bridge] ${req.method} ${url}`);
        console.log(`[API Bridge] Headers: ${JSON.stringify({
            'x-api-key': req.headers['x-api-key'] ? '***' : undefined,
            'anthropic-version': req.headers['anthropic-version'],
            'content-type': req.headers['content-type'],
            'anthropic-beta': req.headers['anthropic-beta'],
        })}`);

        try {
            // Route requests
            if (url === '/' || url === '/health') {
                this.sendJson(res, 200, {
                    status: 'ok',
                    message: 'Copilot API Bridge is running',
                    endpoints: ['/v1/models', '/v1/chat/completions', '/v1/responses', '/v1/messages']
                });
            } else if (url === '/v1/models' && req.method === 'GET') {
                const models = await handleModels();
                this.sendJson(res, 200, models);
            } else if (url.match(/^\/v1\/models\//) && req.method === 'GET') {
                // Handle individual model info requests like /v1/models/claude-opus-4
                const modelId = url.replace('/v1/models/', '');
                this.sendJson(res, 200, {
                    id: modelId,
                    object: 'model',
                    created: Math.floor(Date.now() / 1000),
                    owned_by: 'anthropic',
                    display_name: modelId.replace(/-/g, ' ').replace(/\b\w/g, l => l.toUpperCase()),
                    type: 'model'
                });
            } else if (url === '/v1/account' || url === '/api/account' || url.startsWith('/v1/organizations')) {
                // Fake account/subscription status for Claude Code
                this.sendJson(res, 200, {
                    id: 'org_copilot_bridge',
                    object: 'organization',
                    name: 'Copilot Bridge User',
                    created_at: '2024-01-01T00:00:00Z',
                    subscription: {
                        status: 'active',
                        plan: 'max',
                        tier: 'max'
                    },
                    billing: {
                        status: 'active'
                    },
                    rate_limits: {
                        requests_per_minute: 1000,
                        tokens_per_minute: 100000
                    }
                });
            } else if (url === '/v1/usage' || url === '/api/usage') {
                // Fake usage endpoint
                this.sendJson(res, 200, {
                    object: 'usage',
                    usage: {
                        input_tokens: 0,
                        output_tokens: 0,
                        total_tokens: 0
                    },
                    billing_period: {
                        start: new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString(),
                        end: new Date().toISOString()
                    }
                });
            } else if (url === '/v1/beta/messages' || url.startsWith('/v1/beta/')) {
                // Handle beta endpoints - redirect to regular messages
                if (req.method === 'POST') {
                    const body = await this.readBody(req);
                    const request = JSON.parse(body);
                    await handleMessages(request, res);
                } else {
                    this.sendJson(res, 200, { status: 'ok' });
                }
            } else if (url === '/v1/chat/completions' && req.method === 'POST') {
                const body = await this.readBody(req);
                const request = JSON.parse(body);
                // Use agent handler if tools are provided, otherwise regular handler
                const hasTools = request.tools && request.tools.length > 0;
                console.log(`[API Bridge] hasTools: ${hasTools}, tools count: ${request.tools?.length || 0}`);
                if (hasTools) {
                    console.log(`[API Bridge] Routing to AGENT handler`);
                    await handleAgentChatCompletions(request, res);
                } else {
                    console.log(`[API Bridge] Routing to regular handler`);
                    await handleChatCompletions(request, res);
                }
            } else if (url === '/v1/responses' && req.method === 'POST') {
                const body = await this.readBody(req);
                const request = JSON.parse(body);
                await handleResponses(request, res);
            } else if (url === '/v1/messages/count_tokens' && req.method === 'POST') {
                // Token counting endpoint - estimate tokens
                const body = await this.readBody(req);
                const request = JSON.parse(body);
                const messages = request.messages || [];
                const system = request.system || '';

                // Rough token estimation (4 chars = 1 token)
                let totalChars = system.length;
                for (const msg of messages) {
                    if (typeof msg.content === 'string') {
                        totalChars += msg.content.length;
                    } else if (Array.isArray(msg.content)) {
                        for (const block of msg.content) {
                            if (block.text) totalChars += block.text.length;
                        }
                    }
                }
                const estimatedTokens = Math.ceil(totalChars / 4);

                this.sendJson(res, 200, {
                    input_tokens: estimatedTokens
                });
            } else if (url === '/v1/messages' && req.method === 'POST') {
                const body = await this.readBody(req);
                const request = JSON.parse(body);
                await handleMessages(request, res);
            } else if (url.startsWith('/v1/messages') && req.method === 'POST') {
                // Handle other /v1/messages/* paths
                const body = await this.readBody(req);
                const request = JSON.parse(body);
                await handleMessages(request, res);
            } else {
                // Log unknown endpoints for debugging
                console.log(`[API Bridge] UNKNOWN ENDPOINT: ${req.method} ${url}`);
                if (req.method === 'POST') {
                    try {
                        const body = await this.readBody(req);
                        console.log(`[API Bridge] Body: ${body.substring(0, 500)}`);
                    } catch { /* ignore */ }
                }
                // For unknown GET requests on v1 paths, return empty success
                // This helps with subscription/status checks
                if (req.method === 'GET' && url.startsWith('/v1/')) {
                    this.sendJson(res, 200, {
                        status: 'ok',
                        object: 'list',
                        data: []
                    });
                } else {
                    this.sendJson(res, 404, {
                        error: {
                            message: `Not found: ${url}`,
                            type: 'not_found_error'
                        }
                    });
                }
            }
        } catch (error) {
            console.error('[API Bridge] Error:', error);
            const message = error instanceof Error ? error.message : 'Internal server error';
            this.sendJson(res, 500, {
                error: {
                    message,
                    type: 'internal_error'
                }
            });
        }
    }

    private sendJson(res: http.ServerResponse, status: number, data: unknown): void {
        res.writeHead(status, {
            'Content-Type': 'application/json',
            'request-id': `req_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
            'anthropic-organization-id': 'org_copilot_bridge'
        });
        res.end(JSON.stringify(data));
    }

    private async readBody(req: http.IncomingMessage): Promise<string> {
        return new Promise((resolve, reject) => {
            let body = '';
            req.on('data', chunk => body += chunk);
            req.on('end', () => resolve(body));
            req.on('error', reject);
        });
    }
}



