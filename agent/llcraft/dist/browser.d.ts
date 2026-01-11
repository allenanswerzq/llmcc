/**
 * Browser integration for llcraft
 *
 * Provides browser automation tools by communicating with the chrome package
 * via MCP protocol over stdio.
 */
import { ToolDefinition, ToolCall, ToolResult } from './tools.js';
export declare const browserTools: ToolDefinition[];
export declare const browserToolNames: Set<string>;
/**
 * Browser bridge client - communicates with chrome package via MCP/stdio
 */
export declare class BrowserBridge {
    private chromePath?;
    private process;
    private requestId;
    private pendingRequests;
    private buffer;
    private initialized;
    constructor(chromePath?: string | undefined);
    /**
     * Check if browser bridge is available
     */
    isAvailable(): boolean;
    /**
     * Start the browser bridge process
     */
    start(): Promise<void>;
    /**
     * Stop the browser bridge process
     */
    stop(): Promise<void>;
    /**
     * Execute a browser tool
     */
    executeTool(name: string, args: Record<string, unknown>): Promise<string>;
    /**
     * Send MCP request and wait for response
     */
    private sendRequest;
    /**
     * Handle incoming data from browser bridge
     */
    private handleData;
}
/**
 * Get or create the browser bridge instance
 */
export declare function getBrowserBridge(): BrowserBridge;
/**
 * Check if a tool name is a browser tool
 */
export declare function isBrowserTool(name: string): boolean;
/**
 * Execute a browser tool call
 */
export declare function executeBrowserTool(call: ToolCall): Promise<ToolResult>;
