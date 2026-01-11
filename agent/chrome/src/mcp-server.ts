/**
 * MCP Server
 *
 * Handles MCP (Model Context Protocol) requests and dispatches to tools.
 * This is the main bridge between Claude Code and browser automation.
 */

import { EventEmitter } from 'events';
import { BrowserController } from './browser-controller.js';

export interface MCPRequest {
    id: string | number;
    jsonrpc: '2.0';
    method: string;
    params?: Record<string, unknown>;
}

export interface MCPResponse {
    id: string | number | null;
    jsonrpc: '2.0';
    result?: unknown;
    error?: {
        code: number;
        message: string;
        data?: unknown;
    };
}

export interface ToolDefinition {
    name: string;
    description: string;
    input_schema: {
        type: 'object';
        properties: Record<string, unknown>;
        required?: string[];
    };
}

export class MCPServer extends EventEmitter {
    private browser: BrowserController;
    private activeTabId: string = '';
    private initialized = false;

    constructor(browser: BrowserController) {
        super();
        this.browser = browser;
    }

    /**
     * Initialize the MCP server
     */
    async init(): Promise<void> {
        if (this.initialized) return;

        await this.browser.init();
        const tabs = await this.browser.getTabs();
        if (tabs.length > 0) {
            this.activeTabId = tabs[0].tabId;
        }
        this.initialized = true;
    }

    /**
     * Handle incoming MCP request
     */
    async handleRequest(request: MCPRequest): Promise<MCPResponse> {
        const { id, method, params } = request;

        try {
            let result: unknown;

            switch (method) {
                case 'initialize':
                    await this.init();
                    result = {
                        protocolVersion: '2024-11-05',
                        capabilities: {
                            tools: {},
                        },
                        serverInfo: {
                            name: 'browser-bridge',
                            version: '1.0.0',
                        },
                    };
                    break;

                case 'initialized':
                    result = {};
                    break;

                case 'tools/list':
                    result = { tools: this.getToolDefinitions() };
                    break;

                case 'tools/call':
                    result = await this.callTool(
                        (params as any).name,
                        (params as any).arguments || {}
                    );
                    break;

                case 'shutdown':
                    await this.browser.close();
                    result = {};
                    break;

                default:
                    return {
                        id,
                        jsonrpc: '2.0',
                        error: {
                            code: -32601,
                            message: `Method not found: ${method}`,
                        },
                    };
            }

            return { id, jsonrpc: '2.0', result };
        } catch (error) {
            return {
                id,
                jsonrpc: '2.0',
                error: {
                    code: -32000,
                    message: error instanceof Error ? error.message : String(error),
                },
            };
        }
    }

    /**
     * Get tool definitions for MCP
     */
    private getToolDefinitions(): ToolDefinition[] {
        return [
            {
                name: 'navigate',
                description: 'Navigate to a URL',
                input_schema: {
                    type: 'object',
                    properties: {
                        url: { type: 'string', description: 'URL to navigate to' },
                    },
                    required: ['url'],
                },
            },
            {
                name: 'read_page',
                description: 'Read the current page accessibility tree',
                input_schema: {
                    type: 'object',
                    properties: {
                        depth: { type: 'number', description: 'Max depth of tree', default: 15 },
                    },
                },
            },
            {
                name: 'get_page_text',
                description: 'Get text content of current page',
                input_schema: {
                    type: 'object',
                    properties: {},
                },
            },
            {
                name: 'find',
                description: 'Find interactive elements matching text query',
                input_schema: {
                    type: 'object',
                    properties: {
                        query: { type: 'string', description: 'Text to search for' },
                    },
                    required: ['query'],
                },
            },
            {
                name: 'computer',
                description: 'Perform mouse/keyboard actions',
                input_schema: {
                    type: 'object',
                    properties: {
                        action: {
                            type: 'string',
                            enum: ['screenshot', 'click', 'double_click', 'right_click', 'type', 'key', 'scroll', 'move'],
                            description: 'Action type',
                        },
                        coordinate: {
                            type: 'array',
                            items: { type: 'number' },
                            description: '[x, y] coordinates',
                        },
                        text: { type: 'string', description: 'Text for type action' },
                        key: { type: 'string', description: 'Key for key action' },
                        scroll_direction: {
                            type: 'string',
                            enum: ['up', 'down', 'left', 'right'],
                        },
                        scroll_amount: { type: 'number', default: 3 },
                    },
                    required: ['action'],
                },
            },
            {
                name: 'tabs_create',
                description: 'Create a new browser tab',
                input_schema: {
                    type: 'object',
                    properties: {
                        url: { type: 'string', description: 'Optional URL to open' },
                    },
                },
            },
            {
                name: 'tabs_context',
                description: 'Get current browser context (tabs list)',
                input_schema: {
                    type: 'object',
                    properties: {},
                },
            },
            {
                name: 'form_input',
                description: 'Fill form inputs',
                input_schema: {
                    type: 'object',
                    properties: {
                        selector: { type: 'string', description: 'CSS selector' },
                        value: { type: 'string', description: 'Value to fill' },
                    },
                    required: ['selector', 'value'],
                },
            },
            {
                name: 'javascript_tool',
                description: 'Execute JavaScript in page context',
                input_schema: {
                    type: 'object',
                    properties: {
                        script: { type: 'string', description: 'JavaScript code' },
                    },
                    required: ['script'],
                },
            },
            {
                name: 'read_console_messages',
                description: 'Read browser console messages',
                input_schema: {
                    type: 'object',
                    properties: {
                        limit: { type: 'number', default: 100 },
                        errors_only: { type: 'boolean', default: false },
                    },
                },
            },
            {
                name: 'read_network_requests',
                description: 'Read network requests',
                input_schema: {
                    type: 'object',
                    properties: {
                        limit: { type: 'number', default: 100 },
                        url_pattern: { type: 'string' },
                    },
                },
            },
            {
                name: 'resize_window',
                description: 'Resize browser viewport',
                input_schema: {
                    type: 'object',
                    properties: {
                        width: { type: 'number' },
                        height: { type: 'number' },
                    },
                    required: ['width', 'height'],
                },
            },
        ];
    }

    /**
     * Call a tool
     */
    private async callTool(name: string, args: Record<string, unknown>): Promise<{ content: unknown[] }> {
        switch (name) {
            case 'navigate':
                return this.toolNavigate(args.url as string);

            case 'read_page':
                return this.toolReadPage(args.depth as number | undefined);

            case 'get_page_text':
                return this.toolGetPageText();

            case 'find':
                return this.toolFind(args.query as string);

            case 'computer':
                return this.toolComputer(args);

            case 'tabs_create':
                return this.toolTabsCreate(args.url as string | undefined);

            case 'tabs_context':
                return this.toolTabsContext();

            case 'form_input':
                return this.toolFormInput(args.selector as string, args.value as string);

            case 'javascript_tool':
                return this.toolJavaScript(args.script as string);

            case 'read_console_messages':
                return this.toolReadConsole(args.limit as number, args.errors_only as boolean);

            case 'read_network_requests':
                return this.toolReadNetwork(args.limit as number, args.url_pattern as string);

            case 'resize_window':
                return this.toolResize(args.width as number, args.height as number);

            default:
                throw new Error(`Unknown tool: ${name}`);
        }
    }

    // Tool implementations

    private async toolNavigate(url: string): Promise<{ content: unknown[] }> {
        const result = await this.browser.navigate(this.activeTabId, url);
        return {
            content: [{ type: 'text', text: result }],
        };
    }

    private async toolReadPage(depth?: number): Promise<{ content: unknown[] }> {
        const tree = await this.browser.getAccessibilityTree(this.activeTabId, depth);
        return {
            content: [{ type: 'text', text: tree }],
        };
    }

    private async toolGetPageText(): Promise<{ content: unknown[] }> {
        const text = await this.browser.getPageText(this.activeTabId);
        return {
            content: [{ type: 'text', text }],
        };
    }

    private async toolFind(query: string): Promise<{ content: unknown[] }> {
        const elements = await this.browser.findElements(this.activeTabId, query);
        const formatted = elements.map((el: { role: string; text: string; x: number; y: number }) =>
            `- ${el.role}: "${el.text}" at (${el.x}, ${el.y})`
        ).join('\n');
        return {
            content: [{ type: 'text', text: formatted || 'No elements found' }],
        };
    }

    private async toolComputer(args: Record<string, unknown>): Promise<{ content: unknown[] }> {
        const action = args.action as string;
        const coordinate = args.coordinate as number[] | undefined;

        switch (action) {
            case 'screenshot': {
                const result = await this.browser.screenshot(this.activeTabId);
                return {
                    content: [
                        {
                            type: 'image',
                            data: result.data,
                            mimeType: 'image/jpeg',
                        },
                    ],
                };
            }

            case 'click': {
                if (!coordinate) throw new Error('Click requires coordinate');
                const result = await this.browser.click(this.activeTabId, coordinate[0], coordinate[1]);
                return { content: [{ type: 'text', text: result }] };
            }

            case 'double_click': {
                if (!coordinate) throw new Error('Double click requires coordinate');
                const result = await this.browser.click(this.activeTabId, coordinate[0], coordinate[1], 'left', 2);
                return { content: [{ type: 'text', text: result }] };
            }

            case 'right_click': {
                if (!coordinate) throw new Error('Right click requires coordinate');
                const result = await this.browser.click(this.activeTabId, coordinate[0], coordinate[1], 'right');
                return { content: [{ type: 'text', text: result }] };
            }

            case 'type': {
                const text = args.text as string;
                if (!text) throw new Error('Type requires text');
                const result = await this.browser.type(this.activeTabId, text);
                return { content: [{ type: 'text', text: result }] };
            }

            case 'key': {
                const key = args.key as string;
                if (!key) throw new Error('Key requires key name');
                const result = await this.browser.pressKey(this.activeTabId, key);
                return { content: [{ type: 'text', text: result }] };
            }

            case 'scroll': {
                const direction = args.scroll_direction as 'up' | 'down' | 'left' | 'right';
                const amount = args.scroll_amount as number || 3;
                const x = coordinate?.[0] || 640;
                const y = coordinate?.[1] || 400;
                const result = await this.browser.scroll(this.activeTabId, x, y, direction, amount);
                return { content: [{ type: 'text', text: result }] };
            }

            case 'move': {
                if (!coordinate) throw new Error('Move requires coordinate');
                return { content: [{ type: 'text', text: `Moved to (${coordinate[0]}, ${coordinate[1]})` }] };
            }

            default:
                throw new Error(`Unknown action: ${action}`);
        }
    }

    private async toolTabsCreate(url?: string): Promise<{ content: unknown[] }> {
        const tabId = await this.browser.createTab();
        this.activeTabId = tabId;
        if (url) {
            await this.browser.navigate(tabId, url);
        }
        return {
            content: [{ type: 'text', text: `Created tab ${tabId}` }],
        };
    }

    private async toolTabsContext(): Promise<{ content: unknown[] }> {
        const tabs = await this.browser.getTabs();
        const formatted = tabs.map((t: { tabId: string; title: string; url: string }) =>
            `${t.tabId === this.activeTabId ? '* ' : '  '}${t.tabId}: ${t.title} (${t.url})`
        ).join('\n');
        return {
            content: [{ type: 'text', text: formatted || 'No tabs open' }],
        };
    }

    private async toolFormInput(selector: string, value: string): Promise<{ content: unknown[] }> {
        const result = await this.browser.fillInput(this.activeTabId, selector, value);
        return {
            content: [{ type: 'text', text: result }],
        };
    }

    private async toolJavaScript(script: string): Promise<{ content: unknown[] }> {
        const result = await this.browser.executeScript(this.activeTabId, script);
        return {
            content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
        };
    }

    private async toolReadConsole(limit?: number, errorsOnly?: boolean): Promise<{ content: unknown[] }> {
        const messages = this.browser.getConsoleMessages(this.activeTabId, limit, errorsOnly);
        const formatted = messages.map((m: { type: string; text: string }) => `[${m.type}] ${m.text}`).join('\n');
        return {
            content: [{ type: 'text', text: formatted || 'No console messages' }],
        };
    }

    private async toolReadNetwork(limit?: number, urlPattern?: string): Promise<{ content: unknown[] }> {
        const requests = this.browser.getNetworkRequests(this.activeTabId, limit, urlPattern);
        const formatted = requests.map((r: { method: string; status?: number; url: string }) => `${r.method} ${r.status || '?'} ${r.url}`).join('\n');
        return {
            content: [{ type: 'text', text: formatted || 'No network requests' }],
        };
    }

    private async toolResize(width: number, height: number): Promise<{ content: unknown[] }> {
        const result = await this.browser.setViewport(this.activeTabId, width, height);
        return {
            content: [{ type: 'text', text: result }],
        };
    }
}

export default MCPServer;
