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
        res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');

        // Handle preflight
        if (req.method === 'OPTIONS') {
            res.writeHead(204);
            res.end();
            return;
        }

        const url = req.url || '/';
        console.log(`[API Bridge] ${req.method} ${url}`);

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
            } else if (url.startsWith('/v1/messages') && req.method === 'POST') {
                const body = await this.readBody(req);
                const request = JSON.parse(body);
                await handleMessages(request, res);
            } else {
                this.sendJson(res, 404, {
                    error: {
                        message: `Not found: ${url}`,
                        type: 'not_found_error'
                    }
                });
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
        res.writeHead(status, { 'Content-Type': 'application/json' });
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



