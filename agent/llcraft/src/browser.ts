/**
 * Browser integration for llcraft
 *
 * Provides browser automation tools by communicating with the chrome package
 * via MCP protocol over stdio.
 */

import { spawn, ChildProcess } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { ToolDefinition, ToolCall, ToolResult } from './tools.js';

// Browser tool definitions (subset of chrome package tools)
export const browserTools: ToolDefinition[] = [
    {
        type: 'function',
        function: {
            name: 'navigate',
            description: 'Navigate browser to a URL',
            parameters: {
                type: 'object',
                properties: {
                    url: {
                        type: 'string',
                        description: 'The URL to navigate to',
                    },
                },
                required: ['url'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'read_page',
            description: 'Read the current page accessibility tree',
            parameters: {
                type: 'object',
                properties: {
                    depth: {
                        type: 'number',
                        description: 'Maximum depth of the accessibility tree (default: 3)',
                    },
                },
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'get_page_text',
            description: 'Extract all text content from the current page',
            parameters: {
                type: 'object',
                properties: {},
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'find',
            description: 'Find interactive elements on the page by text query',
            parameters: {
                type: 'object',
                properties: {
                    query: {
                        type: 'string',
                        description: 'Text to search for',
                    },
                },
                required: ['query'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'computer',
            description: 'Perform mouse/keyboard actions or take screenshots',
            parameters: {
                type: 'object',
                properties: {
                    action: {
                        type: 'string',
                        description: 'Action to perform',
                        enum: ['screenshot', 'click', 'double_click', 'right_click', 'type', 'key', 'scroll', 'move'],
                    },
                    coordinate: {
                        type: 'array',
                        description: 'X, Y coordinates for click/move actions',
                    },
                    text: {
                        type: 'string',
                        description: 'Text to type or key to press',
                    },
                },
                required: ['action'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'tabs_create',
            description: 'Create a new browser tab',
            parameters: {
                type: 'object',
                properties: {
                    url: {
                        type: 'string',
                        description: 'Optional URL to open in the new tab',
                    },
                },
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'tabs_context',
            description: 'Get list of open browser tabs',
            parameters: {
                type: 'object',
                properties: {},
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'form_input',
            description: 'Fill a form field by CSS selector',
            parameters: {
                type: 'object',
                properties: {
                    selector: {
                        type: 'string',
                        description: 'CSS selector for the input field',
                    },
                    value: {
                        type: 'string',
                        description: 'Value to enter',
                    },
                },
                required: ['selector', 'value'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'javascript_tool',
            description: 'Execute JavaScript in the page context',
            parameters: {
                type: 'object',
                properties: {
                    script: {
                        type: 'string',
                        description: 'JavaScript code to execute',
                    },
                },
                required: ['script'],
            },
        },
    },
];

// Browser tool names for quick lookup
export const browserToolNames = new Set(browserTools.map(t => t.function.name));

/**
 * Browser bridge client - communicates with chrome package via MCP/stdio
 */
export class BrowserBridge {
    private process: ChildProcess | null = null;
    private requestId = 0;
    private pendingRequests = new Map<number, {
        resolve: (value: unknown) => void;
        reject: (error: Error) => void;
    }>();
    private buffer = '';
    private initialized = false;

    constructor(private chromePath?: string) {
        // Try to find chrome package path
        if (!this.chromePath) {
            // Look relative to llcraft
            const relativePath = path.join(__dirname, '../../chrome/dist/index.js');
            if (fs.existsSync(relativePath)) {
                this.chromePath = relativePath;
            } else {
                // Try node_modules
                const nmPath = path.join(__dirname, '../node_modules/@llmcc/chrome/dist/index.js');
                if (fs.existsSync(nmPath)) {
                    this.chromePath = nmPath;
                }
            }
        }
    }

    /**
     * Check if browser bridge is available
     */
    isAvailable(): boolean {
        return this.chromePath !== undefined && fs.existsSync(this.chromePath);
    }

    /**
     * Start the browser bridge process
     */
    async start(): Promise<void> {
        if (this.process) {
            return; // Already running
        }

        if (!this.chromePath) {
            throw new Error('Chrome package not found. Install it with: npm install @llmcc/chrome');
        }

        return new Promise((resolve, reject) => {
            this.process = spawn('node', [this.chromePath!, '--debug'], {
                stdio: ['pipe', 'pipe', 'inherit'],
            });

            this.process.stdout!.on('data', (data: Buffer) => {
                this.handleData(data);
            });

            this.process.on('error', (error) => {
                reject(error);
            });

            this.process.on('close', (code) => {
                this.process = null;
                this.initialized = false;
                // Reject all pending requests
                for (const [, { reject }] of this.pendingRequests) {
                    reject(new Error(`Browser bridge process exited with code ${code}`));
                }
                this.pendingRequests.clear();
            });

            // Initialize MCP
            this.sendRequest('initialize', {}).then(() => {
                this.initialized = true;
                resolve();
            }).catch(reject);
        });
    }

    /**
     * Stop the browser bridge process
     */
    async stop(): Promise<void> {
        if (!this.process) {
            return;
        }

        try {
            await this.sendRequest('shutdown', {});
        } catch {
            // Ignore errors during shutdown
        }

        this.process.kill();
        this.process = null;
        this.initialized = false;
    }

    /**
     * Execute a browser tool
     */
    async executeTool(name: string, args: Record<string, unknown>): Promise<string> {
        if (!this.initialized) {
            await this.start();
        }

        const result = await this.sendRequest('tools/call', {
            name,
            arguments: args,
        }) as { content: Array<{ type: string; text?: string; data?: string }> };

        // Format result
        if (result.content && Array.isArray(result.content)) {
            return result.content
                .map(c => {
                    if (c.type === 'text') return c.text || '';
                    if (c.type === 'image') return `[Screenshot: base64 image data, ${(c.data?.length || 0)} bytes]`;
                    return JSON.stringify(c);
                })
                .join('\n');
        }

        return JSON.stringify(result, null, 2);
    }

    /**
     * Send MCP request and wait for response
     */
    private sendRequest(method: string, params: Record<string, unknown>): Promise<unknown> {
        return new Promise((resolve, reject) => {
            if (!this.process?.stdin) {
                reject(new Error('Browser bridge not running'));
                return;
            }

            const id = ++this.requestId;
            const request = {
                jsonrpc: '2.0',
                id,
                method,
                params,
            };

            this.pendingRequests.set(id, { resolve, reject });

            // Send with length prefix (native messaging format)
            const json = JSON.stringify(request);
            const lengthBuffer = Buffer.alloc(4);
            lengthBuffer.writeUInt32LE(json.length, 0);
            this.process.stdin.write(lengthBuffer);
            this.process.stdin.write(json);
        });
    }

    /**
     * Handle incoming data from browser bridge
     */
    private handleData(data: Buffer): void {
        this.buffer += data.toString();

        // Parse native messaging format: 4-byte length prefix + JSON
        while (this.buffer.length >= 4) {
            const lengthBuffer = Buffer.from(this.buffer.slice(0, 4), 'binary');
            const length = lengthBuffer.readUInt32LE(0);

            if (this.buffer.length < 4 + length) {
                break; // Wait for more data
            }

            const json = this.buffer.slice(4, 4 + length);
            this.buffer = this.buffer.slice(4 + length);

            try {
                const response = JSON.parse(json);
                const pending = this.pendingRequests.get(response.id);
                if (pending) {
                    this.pendingRequests.delete(response.id);
                    if (response.error) {
                        pending.reject(new Error(response.error.message));
                    } else {
                        pending.resolve(response.result);
                    }
                }
            } catch (e) {
                // Ignore parse errors
            }
        }
    }
}

// Singleton instance
let browserBridge: BrowserBridge | null = null;

/**
 * Get or create the browser bridge instance
 */
export function getBrowserBridge(): BrowserBridge {
    if (!browserBridge) {
        browserBridge = new BrowserBridge();
    }
    return browserBridge;
}

/**
 * Check if a tool name is a browser tool
 */
export function isBrowserTool(name: string): boolean {
    return browserToolNames.has(name);
}

/**
 * Execute a browser tool call
 */
export async function executeBrowserTool(call: ToolCall): Promise<ToolResult> {
    const bridge = getBrowserBridge();
    const name = call.function.name;

    try {
        const args = JSON.parse(call.function.arguments || '{}');
        const result = await bridge.executeTool(name, args);
        return {
            tool_call_id: call.id,
            role: 'tool',
            content: result,
        };
    } catch (error) {
        return {
            tool_call_id: call.id,
            role: 'tool',
            content: `Error: ${error instanceof Error ? error.message : String(error)}`,
        };
    }
}
