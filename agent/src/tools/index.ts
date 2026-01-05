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
// Args:
//   command: string (required) - The command to execute
//   timeout: number (optional) - Timeout in seconds (default: 30)
//   cwd: string (optional) - Working directory
// ============================================
builtinTools.set('bash', async (args) => {
    const command = args.command as string;
    if (!command) {
        return { success: false, output: '', error: 'No command provided' };
    }

    const timeoutSec = (args.timeout as number) || 30;
    const cwd = args.cwd as string || args.working_directory as string || undefined;

    try {
        const result = child_process.execSync(command, {
            encoding: 'utf-8',
            timeout: timeoutSec * 1000,
            maxBuffer: 10 * 1024 * 1024, // 10MB max output
            cwd: cwd,
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
// Args:
//   path: string (required) - File path to read
//   start_line: number (optional) - Start from this line (1-indexed)
//   end_line: number (optional) - End at this line (inclusive)
//   num_lines: number (optional) - Number of lines to read (alternative to end_line)
//   encoding: string (optional) - File encoding (default: utf-8)
// ============================================
builtinTools.set('read_file', async (args) => {
    const filePath = args.path as string || args.file_path as string;
    if (!filePath) {
        return { success: false, output: '', error: 'No file path provided' };
    }

    const startLine = (args.start_line as number) || (args.startLine as number) || 1;
    const endLine = (args.end_line as number) || (args.endLine as number) || undefined;
    const numLines = (args.num_lines as number) || (args.numLines as number) || (args.count as number) || undefined;
    const encoding = (args.encoding as BufferEncoding) || 'utf-8';

    try {
        const content = fs.readFileSync(filePath, encoding);
        const lines = content.split('\n');
        const totalLines = lines.length;

        // Calculate effective end line
        let effectiveEnd = endLine;
        if (!effectiveEnd && numLines) {
            effectiveEnd = startLine + numLines - 1;
        }
        if (!effectiveEnd) {
            effectiveEnd = totalLines;
        }

        // Clamp to valid range
        const start = Math.max(1, startLine) - 1; // Convert to 0-indexed
        const end = Math.min(totalLines, effectiveEnd);

        const selectedLines = lines.slice(start, end);
        const output = selectedLines.join('\n');

        // Add line info header
        const header = `[Lines ${start + 1}-${end} of ${totalLines}]\n`;
        return { success: true, output: header + output };
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
// Args:
//   path: string (required) - File path to write
//   content: string (required) - Content to write
//   mode: string (optional) - 'overwrite' (default), 'append', 'insert'
//   line: number (optional) - For 'insert' mode, insert at this line
//   create_dirs: boolean (optional) - Create parent directories (default: true)
// ============================================
builtinTools.set('write_file', async (args) => {
    const filePath = args.path as string || args.file_path as string;
    const content = args.content as string;
    const mode = (args.mode as string) || 'overwrite';
    const insertLine = (args.line as number) || (args.insert_line as number) || 1;
    const createDirs = args.create_dirs !== false && args.createDirs !== false;

    if (!filePath) {
        return { success: false, output: '', error: 'No file path provided' };
    }
    if (content === undefined) {
        return { success: false, output: '', error: 'No content provided' };
    }

    try {
        // Ensure directory exists
        if (createDirs) {
            const dir = path.dirname(filePath);
            if (!fs.existsSync(dir)) {
                fs.mkdirSync(dir, { recursive: true });
            }
        }

        if (mode === 'append') {
            fs.appendFileSync(filePath, content, 'utf-8');
            return { success: true, output: `Appended ${content.length} bytes to ${filePath}` };
        } else if (mode === 'insert' && fs.existsSync(filePath)) {
            const existing = fs.readFileSync(filePath, 'utf-8');
            const lines = existing.split('\n');
            const insertAt = Math.max(0, Math.min(lines.length, insertLine - 1));
            lines.splice(insertAt, 0, content);
            fs.writeFileSync(filePath, lines.join('\n'), 'utf-8');
            return { success: true, output: `Inserted content at line ${insertAt + 1} in ${filePath}` };
        } else {
            fs.writeFileSync(filePath, content, 'utf-8');
            return { success: true, output: `Wrote ${content.length} bytes to ${filePath}` };
        }
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
// Args:
//   path: string (optional) - Directory path (default: '.')
//   recursive: boolean (optional) - List recursively (default: false)
//   max_depth: number (optional) - Max depth for recursive (default: 3)
//   pattern: string (optional) - Glob pattern to filter (e.g., '*.ts')
//   show_hidden: boolean (optional) - Show hidden files (default: false)
// ============================================
builtinTools.set('list_dir', async (args) => {
    const dirPath = args.path as string || args.directory as string || '.';
    const recursive = args.recursive === true;
    const maxDepth = (args.max_depth as number) || (args.maxDepth as number) || 3;
    const pattern = args.pattern as string || args.filter as string;
    const showHidden = args.show_hidden === true || args.showHidden === true;

    try {
        const results: string[] = [];

        function listDir(currentPath: string, depth: number, prefix: string = '') {
            if (recursive && depth > maxDepth) return;

            const entries = fs.readdirSync(currentPath, { withFileTypes: true });
            for (const entry of entries) {
                // Skip hidden files unless requested
                if (!showHidden && entry.name.startsWith('.')) continue;

                // Filter by pattern if provided
                if (pattern) {
                    const regex = new RegExp(pattern.replace(/\*/g, '.*').replace(/\?/g, '.'));
                    if (!regex.test(entry.name)) {
                        if (!entry.isDirectory()) continue;
                    }
                }

                const type = entry.isDirectory() ? 'd' : entry.isSymbolicLink() ? 'l' : '-';
                const fullPath = path.join(currentPath, entry.name);

                if (recursive) {
                    const relativePath = path.relative(dirPath, fullPath);
                    results.push(`${type} ${relativePath}${entry.isDirectory() ? '/' : ''}`);
                    if (entry.isDirectory()) {
                        listDir(fullPath, depth + 1, prefix + '  ');
                    }
                } else {
                    results.push(`${type} ${entry.name}${entry.isDirectory() ? '/' : ''}`);
                }
            }
        }

        listDir(dirPath, 0);
        const output = results.length > 0 ? results.join('\n') : '(empty directory)';
        return { success: true, output: `[${dirPath}]\n${output}` };
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
// Args:
//   pattern: string (required) - Search pattern (regex)
//   path: string (optional) - Directory or file to search (default: '.')
//   include: string (optional) - File pattern to include (e.g., '*.ts')
//   exclude: string (optional) - File pattern to exclude (e.g., 'node_modules')
//   max_results: number (optional) - Maximum results (default: 50)
//   context_lines: number (optional) - Lines of context before/after match (default: 0)
//   case_sensitive: boolean (optional) - Case sensitive search (default: false)
// ============================================
builtinTools.set('grep_search', async (args) => {
    const pattern = args.pattern as string || args.query as string;
    const searchPath = args.path as string || args.directory as string || '.';
    const include = args.include as string || args.file_pattern as string;
    const exclude = args.exclude as string || 'node_modules';
    const maxResults = (args.max_results as number) || (args.maxResults as number) || 50;
    const contextLines = (args.context_lines as number) || (args.contextLines as number) || 0;
    const caseSensitive = args.case_sensitive === true || args.caseSensitive === true;

    if (!pattern) {
        return { success: false, output: '', error: 'No search pattern provided' };
    }

    try {
        let command: string;

        if (process.platform === 'win32') {
            // PowerShell command
            let psCommand = `Get-ChildItem -Path "${searchPath}" -Recurse -File`;
            if (include) {
                psCommand += ` -Include "${include}"`;
            }
            if (exclude) {
                psCommand += ` | Where-Object { $_.FullName -notmatch '${exclude}' }`;
            }
            const caseFlag = caseSensitive ? '' : 'i';
            psCommand += ` | Select-String -Pattern "${pattern}" ${caseSensitive ? '-CaseSensitive' : ''}`;
            if (contextLines > 0) {
                psCommand += ` -Context ${contextLines}`;
            }
            psCommand += ` | Select-Object -First ${maxResults}`;
            command = psCommand;
        } else {
            // Unix grep command
            const caseFlag = caseSensitive ? '' : '-i';
            const contextFlag = contextLines > 0 ? `-C ${contextLines}` : '';
            const includeFlag = include ? `--include="${include}"` : '';
            const excludeFlag = exclude ? `--exclude-dir="${exclude}"` : '';
            command = `grep -rn ${caseFlag} ${contextFlag} ${includeFlag} ${excludeFlag} "${pattern}" "${searchPath}" | head -${maxResults}`;
        }

        const result = child_process.execSync(command, {
            encoding: 'utf-8',
            timeout: 30000,
            maxBuffer: 10 * 1024 * 1024,
            shell: process.platform === 'win32' ? 'powershell.exe' : '/bin/bash'
        });
        return { success: true, output: result || 'No matches found' };
    } catch (error) {
        const execError = error as child_process.ExecException & { stdout?: string };
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
// Args:
//   path: string (required) - Path to check
//   get_stats: boolean (optional) - Return detailed file stats (default: false)
// ============================================
builtinTools.set('file_exists', async (args) => {
    const filePath = args.path as string;
    const getStats = args.get_stats === true || args.getStats === true;

    if (!filePath) {
        return { success: false, output: '', error: 'No path provided' };
    }

    try {
        const exists = fs.existsSync(filePath);
        if (!exists) {
            return { success: true, output: `${filePath} does not exist` };
        }

        const stats = fs.statSync(filePath);
        const type = stats.isDirectory() ? 'directory' : stats.isFile() ? 'file' : stats.isSymbolicLink() ? 'symlink' : 'unknown';

        if (getStats) {
            const info = {
                type,
                size: stats.size,
                created: stats.birthtime.toISOString(),
                modified: stats.mtime.toISOString(),
                permissions: stats.mode.toString(8).slice(-3)
            };
            return { success: true, output: JSON.stringify(info, null, 2) };
        }

        return { success: true, output: `${type} exists at ${filePath} (${stats.size} bytes)` };
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
// Args:
//   path: string (required) - File path to edit
//   pattern: string (required) - Regex pattern to find
//   replacement: string (optional) - Replacement string (default: '')
//   global: boolean (optional) - Replace all occurrences (default: true)
//   dry_run: boolean (optional) - Show changes without applying (default: false)
//   backup: boolean (optional) - Create backup file (default: false)
// ============================================
builtinTools.set('sed', async (args) => {
    const filePath = args.file as string || args.path as string;
    const pattern = args.pattern as string;
    const replacement = (args.replacement as string) ?? '';
    const globalReplace = args.global !== false;
    const dryRun = args.dry_run === true || args.dryRun === true;
    const backup = args.backup === true;

    if (!filePath || !pattern) {
        return { success: false, output: '', error: 'File path and pattern are required' };
    }

    try {
        const content = fs.readFileSync(filePath, 'utf-8');
        const regex = new RegExp(pattern, globalReplace ? 'g' : '');
        const matches = content.match(regex) || [];
        const changes = matches.length;

        if (changes === 0) {
            return { success: true, output: `No matches found for pattern: ${pattern}` };
        }

        const newContent = content.replace(regex, replacement);

        if (dryRun) {
            // Show diff-like output
            const lines = content.split('\n');
            const newLines = newContent.split('\n');
            const diff: string[] = [`Would replace ${changes} occurrence(s):`];

            for (let i = 0; i < Math.max(lines.length, newLines.length); i++) {
                if (lines[i] !== newLines[i]) {
                    diff.push(`Line ${i + 1}:`);
                    diff.push(`  - ${lines[i]}`);
                    diff.push(`  + ${newLines[i]}`);
                }
            }
            return { success: true, output: diff.slice(0, 50).join('\n') };
        }

        if (backup) {
            fs.writeFileSync(filePath + '.bak', content, 'utf-8');
        }

        fs.writeFileSync(filePath, newContent, 'utf-8');
        return { success: true, output: `Replaced ${changes} occurrence(s) in ${filePath}` };
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

// ============================================
// Built-in Tool Documentation
// ============================================
export interface BuiltinToolDoc {
    name: string;
    description: string;
    parameters: {
        name: string;
        type: string;
        required: boolean;
        description: string;
        default?: string;
    }[];
    examples: string[];
}

export function getBuiltinToolDocs(): BuiltinToolDoc[] {
    return [
        {
            name: 'bash',
            description: 'Execute shell commands (PowerShell on Windows, bash on Linux/Mac)',
            parameters: [
                { name: 'command', type: 'string', required: true, description: 'The command to execute' },
                { name: 'timeout', type: 'number', required: false, description: 'Timeout in seconds', default: '30' },
                { name: 'cwd', type: 'string', required: false, description: 'Working directory for the command' },
            ],
            examples: [
                '{"tool": "bash", "command": "ls -la"}',
                '{"tool": "bash", "command": "npm install", "cwd": "/project", "timeout": 120}',
            ]
        },
        {
            name: 'read_file',
            description: 'Read file contents with optional line range',
            parameters: [
                { name: 'path', type: 'string', required: true, description: 'File path to read' },
                { name: 'start_line', type: 'number', required: false, description: 'Start from this line (1-indexed)', default: '1' },
                { name: 'end_line', type: 'number', required: false, description: 'End at this line (inclusive)' },
                { name: 'num_lines', type: 'number', required: false, description: 'Number of lines to read (alternative to end_line)' },
                { name: 'encoding', type: 'string', required: false, description: 'File encoding', default: 'utf-8' },
            ],
            examples: [
                '{"tool": "read_file", "path": "/src/main.ts"}',
                '{"tool": "read_file", "path": "/src/main.ts", "start_line": 100, "num_lines": 50}',
                '{"tool": "read_file", "path": "/src/main.ts", "start_line": 50, "end_line": 100}',
            ]
        },
        {
            name: 'write_file',
            description: 'Write or modify file contents',
            parameters: [
                { name: 'path', type: 'string', required: true, description: 'File path to write' },
                { name: 'content', type: 'string', required: true, description: 'Content to write' },
                { name: 'mode', type: 'string', required: false, description: 'Write mode: overwrite, append, or insert', default: 'overwrite' },
                { name: 'line', type: 'number', required: false, description: 'For insert mode, insert at this line' },
                { name: 'create_dirs', type: 'boolean', required: false, description: 'Create parent directories if missing', default: 'true' },
            ],
            examples: [
                '{"tool": "write_file", "path": "/test.txt", "content": "Hello World"}',
                '{"tool": "write_file", "path": "/log.txt", "content": "New log entry\\n", "mode": "append"}',
                '{"tool": "write_file", "path": "/src/main.ts", "content": "// New comment", "mode": "insert", "line": 1}',
            ]
        },
        {
            name: 'list_dir',
            description: 'List directory contents with optional recursion and filtering',
            parameters: [
                { name: 'path', type: 'string', required: false, description: 'Directory path', default: '.' },
                { name: 'recursive', type: 'boolean', required: false, description: 'List recursively', default: 'false' },
                { name: 'max_depth', type: 'number', required: false, description: 'Max depth for recursive listing', default: '3' },
                { name: 'pattern', type: 'string', required: false, description: 'Filter by pattern (e.g., *.ts)' },
                { name: 'show_hidden', type: 'boolean', required: false, description: 'Show hidden files', default: 'false' },
            ],
            examples: [
                '{"tool": "list_dir", "path": "/src"}',
                '{"tool": "list_dir", "path": "/src", "recursive": true, "pattern": "*.ts"}',
                '{"tool": "list_dir", "path": ".", "recursive": true, "max_depth": 2, "show_hidden": true}',
            ]
        },
        {
            name: 'grep_search',
            description: 'Search for text patterns in files',
            parameters: [
                { name: 'pattern', type: 'string', required: true, description: 'Search pattern (supports regex)' },
                { name: 'path', type: 'string', required: false, description: 'Directory or file to search', default: '.' },
                { name: 'include', type: 'string', required: false, description: 'File pattern to include (e.g., *.ts)' },
                { name: 'exclude', type: 'string', required: false, description: 'Pattern to exclude', default: 'node_modules' },
                { name: 'max_results', type: 'number', required: false, description: 'Maximum number of results', default: '50' },
                { name: 'context_lines', type: 'number', required: false, description: 'Lines of context around matches', default: '0' },
                { name: 'case_sensitive', type: 'boolean', required: false, description: 'Case sensitive search', default: 'false' },
            ],
            examples: [
                '{"tool": "grep_search", "pattern": "TODO", "path": "/src"}',
                '{"tool": "grep_search", "pattern": "function.*export", "include": "*.ts", "context_lines": 2}',
                '{"tool": "grep_search", "pattern": "error", "path": "/logs", "case_sensitive": true}',
            ]
        },
        {
            name: 'file_exists',
            description: 'Check if a file or directory exists',
            parameters: [
                { name: 'path', type: 'string', required: true, description: 'Path to check' },
                { name: 'get_stats', type: 'boolean', required: false, description: 'Return detailed file stats (size, dates, permissions)', default: 'false' },
            ],
            examples: [
                '{"tool": "file_exists", "path": "/src/main.ts"}',
                '{"tool": "file_exists", "path": "/config.json", "get_stats": true}',
            ]
        },
        {
            name: 'sed',
            description: 'Find and replace text in files using regex',
            parameters: [
                { name: 'path', type: 'string', required: true, description: 'File path to edit' },
                { name: 'pattern', type: 'string', required: true, description: 'Regex pattern to find' },
                { name: 'replacement', type: 'string', required: false, description: 'Replacement string', default: '' },
                { name: 'global', type: 'boolean', required: false, description: 'Replace all occurrences', default: 'true' },
                { name: 'dry_run', type: 'boolean', required: false, description: 'Preview changes without applying', default: 'false' },
                { name: 'backup', type: 'boolean', required: false, description: 'Create .bak backup file', default: 'false' },
            ],
            examples: [
                '{"tool": "sed", "path": "/file.ts", "pattern": "oldText", "replacement": "newText"}',
                '{"tool": "sed", "path": "/file.ts", "pattern": "console\\\\.log\\\\(.*\\\\)", "replacement": "", "dry_run": true}',
                '{"tool": "sed", "path": "/config.json", "pattern": "localhost", "replacement": "production.server.com", "backup": true}',
            ]
        },
    ];
}

// Format tool docs for system prompt
export function formatBuiltinToolsForPrompt(): string {
    const docs = getBuiltinToolDocs();
    return docs.map(tool => {
        const params = tool.parameters.map(p => {
            const req = p.required ? '(required)' : `(optional, default: ${p.default || 'none'})`;
            return `    - ${p.name}: ${p.type} ${req} - ${p.description}`;
        }).join('\n');

        const examples = tool.examples.map(e => `    ${e}`).join('\n');

        return `${tool.name}: ${tool.description}
  Parameters:
${params}
  Examples:
${examples}`;
    }).join('\n\n');
}
