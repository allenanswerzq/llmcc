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
export declare class MCPServer extends EventEmitter {
    private browser;
    private activeTabId;
    private initialized;
    constructor(browser: BrowserController);
    /**
     * Initialize the MCP server
     */
    init(): Promise<void>;
    /**
     * Handle incoming MCP request
     */
    handleRequest(request: MCPRequest): Promise<MCPResponse>;
    /**
     * Get tool definitions for MCP
     */
    private getToolDefinitions;
    /**
     * Call a tool
     */
    private callTool;
    private toolNavigate;
    private toolReadPage;
    private toolGetPageText;
    private toolFind;
    private toolComputer;
    private toolTabsCreate;
    private toolTabsContext;
    private toolFormInput;
    private toolJavaScript;
    private toolReadConsole;
    private toolReadNetwork;
    private toolResize;
}
export default MCPServer;
//# sourceMappingURL=mcp-server.d.ts.map