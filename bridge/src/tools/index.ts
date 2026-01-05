// Built-in tool execution system for the Copilot API Bridge

import * as fs from 'fs';
import * as path from 'path';
import * as child_process from 'child_process';

export interface ToolDefinition {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
}

export interface ToolCall {
    name: string;
    arguments: Record<string, unknown>;
}

export interface ToolResult {
    success: boolean;
    output: string;
    error?: string;
}

// Registry of built-in tools
const builtinTools: Map<string, (args: Record<string, unknown>) => Promise<ToolResult>> = new Map();

// ============================================
// Tool: bash - Execute shell commands
// ============================================
builtinTools.set('bash', async (args) => {
    const command = args.command as string;
    if (!command) {
        return { success: false, output: '', error: 'No command provided' };
    }

    try {
        const result = child_process.execSync(command, {
            encoding: 'utf-8',
            timeout: 30000, // 30 second timeout
            maxBuffer: 1024 * 1024, // 1MB max output
            shell: process.platform === 'win32' ? 'powershell.exe' : '/bin/bash'
        });
        return { success: true, output: result };
    } catch (error) {
        const execError = error as child_process.ExecException & { stdout?: string; stderr?: string };
        return {
            success: false,
            output: execError.stdout || '',
            error: execError.stderr || execError.message
        };
    }
});

// ============================================
// Tool: read_file - Read file contents
// ============================================
builtinTools.set('read_file', async (args) => {
    const filePath = args.path as string || args.file_path as string;
    if (!filePath) {
        return { success: false, output: '', error: 'No file path provided' };
    }

    try {
        const content = fs.readFileSync(filePath, 'utf-8');
        return { success: true, output: content };
    } catch (error) {
        return {
            success: false,
            output: '',
            error: `Failed to read file: ${(error as Error).message}`
        };
    }
});

// ============================================
// Tool: write_file - Write content to file
// ============================================
builtinTools.set('write_file', async (args) => {
    const filePath = args.path as string || args.file_path as string;
    const content = args.content as string;

    if (!filePath) {
        return { success: false, output: '', error: 'No file path provided' };
    }
    if (content === undefined) {
        return { success: false, output: '', error: 'No content provided' };
    }

    try {
        // Ensure directory exists
        const dir = path.dirname(filePath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }

        fs.writeFileSync(filePath, content, 'utf-8');
        return { success: true, output: `Successfully wrote ${content.length} bytes to ${filePath}` };
    } catch (error) {
        return {
            success: false,
            output: '',
            error: `Failed to write file: ${(error as Error).message}`
        };
    }
});

// ============================================
// Tool: list_dir - List directory contents
// ============================================
builtinTools.set('list_dir', async (args) => {
    const dirPath = args.path as string || args.directory as string || '.';

    try {
        const entries = fs.readdirSync(dirPath, { withFileTypes: true });
        const output = entries.map(entry => {
            const type = entry.isDirectory() ? 'd' : entry.isSymbolicLink() ? 'l' : '-';
            return `${type} ${entry.name}`;
        }).join('\n');
        return { success: true, output };
    } catch (error) {
        return {
            success: false,
            output: '',
            error: `Failed to list directory: ${(error as Error).message}`
        };
    }
});

// ============================================
// Tool: grep_search - Search for text in files
// ============================================
builtinTools.set('grep_search', async (args) => {
    const pattern = args.pattern as string || args.query as string;
    const searchPath = args.path as string || args.directory as string || '.';

    if (!pattern) {
        return { success: false, output: '', error: 'No search pattern provided' };
    }

    try {
        // Use grep on Unix, findstr on Windows
        const command = process.platform === 'win32'
            ? `Get-ChildItem -Path "${searchPath}" -Recurse -File | Select-String -Pattern "${pattern}" | Select-Object -First 50`
            : `grep -rn "${pattern}" "${searchPath}" | head -50`;

        const result = child_process.execSync(command, {
            encoding: 'utf-8',
            timeout: 30000,
            maxBuffer: 1024 * 1024,
            shell: process.platform === 'win32' ? 'powershell.exe' : '/bin/bash'
        });
        return { success: true, output: result || 'No matches found' };
    } catch (error) {
        const execError = error as child_process.ExecException & { stdout?: string };
        // grep returns exit code 1 when no matches found
        if (execError.stdout !== undefined) {
            return { success: true, output: execError.stdout || 'No matches found' };
        }
        return {
            success: false,
            output: '',
            error: `Search failed: ${(error as Error).message}`
        };
    }
});

// ============================================
// Tool: file_exists - Check if file/directory exists
// ============================================
builtinTools.set('file_exists', async (args) => {
    const filePath = args.path as string;
    if (!filePath) {
        return { success: false, output: '', error: 'No path provided' };
    }

    try {
        const exists = fs.existsSync(filePath);
        const stats = exists ? fs.statSync(filePath) : null;
        const type = stats?.isDirectory() ? 'directory' : stats?.isFile() ? 'file' : 'unknown';
        return {
            success: true,
            output: exists ? `${type} exists at ${filePath}` : `${filePath} does not exist`
        };
    } catch (error) {
        return {
            success: false,
            output: '',
            error: `Failed to check path: ${(error as Error).message}`
        };
    }
});

// ============================================
// Tool: sed - Find and replace in file
// ============================================
builtinTools.set('sed', async (args) => {
    const filePath = args.file as string || args.path as string;
    const pattern = args.pattern as string;
    const replacement = args.replacement as string;

    if (!filePath || !pattern) {
        return { success: false, output: '', error: 'File path and pattern are required' };
    }

    try {
        let content = fs.readFileSync(filePath, 'utf-8');
        const regex = new RegExp(pattern, 'g');
        const newContent = content.replace(regex, replacement || '');
        const changes = (content.match(regex) || []).length;

        if (changes > 0) {
            fs.writeFileSync(filePath, newContent, 'utf-8');
            return { success: true, output: `Replaced ${changes} occurrence(s) in ${filePath}` };
        } else {
            return { success: true, output: `No matches found for pattern: ${pattern}` };
        }
    } catch (error) {
        return {
            success: false,
            output: '',
            error: `sed failed: ${(error as Error).message}`
        };
    }
});

// ============================================
// Tool Execution
// ============================================

export async function executeTool(name: string, args: Record<string, unknown>): Promise<ToolResult> {
    const tool = builtinTools.get(name);
    if (!tool) {
        return {
            success: false,
            output: '',
            error: `Unknown tool: ${name}. Available tools: ${Array.from(builtinTools.keys()).join(', ')}`
        };
    }

    console.log(`[Tools] Executing: ${name}(${JSON.stringify(args)})`);
    const result = await tool(args);
    console.log(`[Tools] Result: ${result.success ? 'success' : 'error'}`);
    return result;
}

export function getAvailableTools(): string[] {
    return Array.from(builtinTools.keys());
}

export function isToolAvailable(name: string): boolean {
    return builtinTools.has(name);
}

// ============================================
// Tool Call Parsing from Model Output
// ============================================

// Pattern to detect tool calls in model output
// Supports various formats the model might output

export function parseToolCalls(text: string): ToolCall[] {
    const toolCalls: ToolCall[] = [];

    // Try to extract JSON from code blocks first
    const codeBlockPattern = /```(?:json)?\s*(\{[\s\S]*?\})\s*```/g;
    let match;

    while ((match = codeBlockPattern.exec(text)) !== null) {
        try {
            const obj = JSON.parse(match[1]);
            const toolCall = extractToolCall(obj);
            if (toolCall) {
                toolCalls.push(toolCall);
            }
        } catch {
            // JSON parse failed, skip
        }
    }

    // Also try to find raw JSON objects in the text
    const jsonPattern = /\{[^{}]*"tool"\s*:\s*"[^"]+"\s*[^{}]*\}/g;
    while ((match = jsonPattern.exec(text)) !== null) {
        try {
            const obj = JSON.parse(match[0]);
            const toolCall = extractToolCall(obj);
            if (toolCall && !toolCalls.some(tc => tc.name === toolCall.name)) {
                toolCalls.push(toolCall);
            }
        } catch {
            // JSON parse failed, skip
        }
    }

    return toolCalls;
}

// Extract tool call from a JSON object, handling various formats
function extractToolCall(obj: Record<string, unknown>): ToolCall | null {
    const name = (obj.tool || obj.name || obj.function) as string | undefined;

    if (!name || typeof name !== 'string') {
        return null;
    }

    if (!isToolAvailable(name)) {
        console.log(`[Tools] Tool "${name}" not available locally`);
        return null;
    }

    // Try to get arguments from various possible fields
    let args: Record<string, unknown> = {};

    if (obj.arguments && typeof obj.arguments === 'object') {
        args = obj.arguments as Record<string, unknown>;
    } else if (obj.input && typeof obj.input === 'object') {
        args = obj.input as Record<string, unknown>;
    } else if (obj.args && typeof obj.args === 'object') {
        args = obj.args as Record<string, unknown>;
    } else {
        // Arguments might be at top level - extract all non-tool fields
        for (const [key, value] of Object.entries(obj)) {
            if (key !== 'tool' && key !== 'name' && key !== 'function') {
                args[key] = value;
            }
        }
    }

    return { name, arguments: args };
}

// Generate system prompt for tool usage
export function generateToolSystemPrompt(tools: ToolDefinition[]): string {
    const toolDescriptions = tools.map(t =>
        `- ${t.name}: ${t.description}\n  Parameters: ${JSON.stringify(t.parameters)}`
    ).join('\n');

    return `You have access to the following tools:

${toolDescriptions}

To use a tool, output a JSON object in this exact format:
{"tool": "tool_name", "arguments": {"param1": "value1"}}

After receiving a tool result, continue your response based on the result.
Only call one tool at a time. Wait for the result before calling another tool.`;
}
