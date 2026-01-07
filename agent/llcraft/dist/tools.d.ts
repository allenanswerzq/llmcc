/**
 * Tool system for llaude
 * Define and execute custom tools that the LLM can call
 */
export interface ToolDefinition {
    type: 'function';
    function: {
        name: string;
        description: string;
        parameters: {
            type: 'object';
            properties: Record<string, {
                type: string;
                description: string;
                enum?: string[];
            }>;
            required?: string[];
        };
    };
}
export interface ToolCall {
    id: string;
    type: 'function';
    function: {
        name: string;
        arguments: string;
    };
}
export interface ToolResult {
    tool_call_id: string;
    role: 'tool';
    content: string;
}
export declare const builtinTools: ToolDefinition[];
/**
 * Execute a tool call and return the result
 */
export declare function executeTool(toolCall: ToolCall): ToolResult;
/**
 * Format tools for display
 */
export declare function formatToolList(includeBrowser?: boolean): string;
