/**
 * Tool system for llaude
 * Define and execute custom tools that the LLM can call
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync, spawn } from 'child_process';
import { applyPatch, parsePatch, createPatch } from 'diff';

// Tool definition following OpenAI function calling format
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

// Built-in tools
export const builtinTools: ToolDefinition[] = [
    {
        type: 'function',
        function: {
            name: 'read_file',
            description: 'Read the contents of a file',
            parameters: {
                type: 'object',
                properties: {
                    path: {
                        type: 'string',
                        description: 'The path to the file to read',
                    },
                    startLine: {
                        type: 'number',
                        description: 'Optional start line (1-indexed)',
                    },
                    endLine: {
                        type: 'number',
                        description: 'Optional end line (1-indexed)',
                    },
                },
                required: ['path'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'write_file',
            description: 'Write content to a file (creates or overwrites)',
            parameters: {
                type: 'object',
                properties: {
                    path: {
                        type: 'string',
                        description: 'The path to the file to write',
                    },
                    content: {
                        type: 'string',
                        description: 'The content to write to the file',
                    },
                },
                required: ['path', 'content'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'list_dir',
            description: 'List contents of a directory',
            parameters: {
                type: 'object',
                properties: {
                    path: {
                        type: 'string',
                        description: 'The directory path to list',
                    },
                },
                required: ['path'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'run_command',
            description: 'Run a shell command and return output',
            parameters: {
                type: 'object',
                properties: {
                    command: {
                        type: 'string',
                        description: 'The shell command to execute',
                    },
                    cwd: {
                        type: 'string',
                        description: 'Working directory for the command',
                    },
                },
                required: ['command'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'search_files',
            description: 'Search for files matching a pattern using grep',
            parameters: {
                type: 'object',
                properties: {
                    pattern: {
                        type: 'string',
                        description: 'The regex pattern to search for',
                    },
                    path: {
                        type: 'string',
                        description: 'The directory to search in',
                    },
                    filePattern: {
                        type: 'string',
                        description: 'File glob pattern (e.g., "*.ts")',
                    },
                },
                required: ['pattern'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'edit_file',
            description: 'Replace a specific string in a file',
            parameters: {
                type: 'object',
                properties: {
                    path: {
                        type: 'string',
                        description: 'The path to the file to edit',
                    },
                    oldString: {
                        type: 'string',
                        description: 'The exact string to replace',
                    },
                    newString: {
                        type: 'string',
                        description: 'The replacement string',
                    },
                },
                required: ['path', 'oldString', 'newString'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'apply_patch',
            description: 'Apply a unified diff patch to a file. Supports standard unified diff format (like git diff or diff -u output). Can handle fuzzy matching for context.',
            parameters: {
                type: 'object',
                properties: {
                    path: {
                        type: 'string',
                        description: 'The path to the file to patch',
                    },
                    patch: {
                        type: 'string',
                        description: 'The unified diff patch content to apply',
                    },
                    fuzzFactor: {
                        type: 'number',
                        description: 'Maximum number of lines that can mismatch in context (default: 2)',
                    },
                },
                required: ['path', 'patch'],
            },
        },
    },
    {
        type: 'function',
        function: {
            name: 'create_patch',
            description: 'Create a unified diff patch between old and new content for a file',
            parameters: {
                type: 'object',
                properties: {
                    path: {
                        type: 'string',
                        description: 'The file path (used in patch header)',
                    },
                    oldContent: {
                        type: 'string',
                        description: 'The original content',
                    },
                    newContent: {
                        type: 'string',
                        description: 'The new content',
                    },
                },
                required: ['path', 'oldContent', 'newContent'],
            },
        },
    },
];

/**
 * Execute a tool call and return the result
 */
export function executeTool(toolCall: ToolCall): ToolResult {
    const { name, arguments: argsJson } = toolCall.function;

    try {
        const args = JSON.parse(argsJson);
        let result: string;

        switch (name) {
            case 'read_file':
                result = executeReadFile(args.path, args.startLine, args.endLine);
                break;
            case 'write_file':
                result = executeWriteFile(args.path, args.content);
                break;
            case 'list_dir':
                result = executeListDir(args.path);
                break;
            case 'run_command':
                result = executeRunCommand(args.command, args.cwd);
                break;
            case 'search_files':
                result = executeSearchFiles(args.pattern, args.path, args.filePattern);
                break;
            case 'edit_file':
                result = executeEditFile(args.path, args.oldString, args.newString);
                break;
            case 'apply_patch':
                result = executeApplyPatch(args.path, args.patch, args.fuzzFactor);
                break;
            case 'create_patch':
                result = executeCreatePatch(args.path, args.oldContent, args.newContent);
                break;
            default:
                result = `Unknown tool: ${name}`;
        }

        return {
            tool_call_id: toolCall.id,
            role: 'tool',
            content: result,
        };
    } catch (error) {
        return {
            tool_call_id: toolCall.id,
            role: 'tool',
            content: `Error executing tool: ${error instanceof Error ? error.message : error}`,
        };
    }
}

function executeReadFile(filePath: string, startLine?: number, endLine?: number): string {
    const absolutePath = path.resolve(filePath);

    if (!fs.existsSync(absolutePath)) {
        return `File not found: ${absolutePath}`;
    }

    const content = fs.readFileSync(absolutePath, 'utf-8');
    const lines = content.split('\n');

    if (startLine !== undefined || endLine !== undefined) {
        const start = (startLine || 1) - 1;
        const end = endLine || lines.length;
        return lines.slice(start, end).join('\n');
    }

    // Truncate if too long
    if (content.length > 50000) {
        return content.slice(0, 50000) + '\n... (truncated, file too large)';
    }

    return content;
}

function executeWriteFile(filePath: string, content: string): string {
    const absolutePath = path.resolve(filePath);
    const dir = path.dirname(absolutePath);

    if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
    }

    fs.writeFileSync(absolutePath, content);
    return `File written: ${absolutePath} (${content.length} bytes)`;
}

function executeListDir(dirPath: string): string {
    const absolutePath = path.resolve(dirPath || '.');

    if (!fs.existsSync(absolutePath)) {
        return `Directory not found: ${absolutePath}`;
    }

    const entries = fs.readdirSync(absolutePath, { withFileTypes: true });
    const formatted = entries.map(entry => {
        const suffix = entry.isDirectory() ? '/' : '';
        return entry.name + suffix;
    });

    return formatted.join('\n');
}

function executeRunCommand(command: string, cwd?: string): string {
    try {
        const output = execSync(command, {
            cwd: cwd || process.cwd(),
            encoding: 'utf-8',
            timeout: 30000,
            maxBuffer: 1024 * 1024,
        });
        return output || '(no output)';
    } catch (error: any) {
        if (error.stdout || error.stderr) {
            return `Exit code: ${error.status}\nstdout: ${error.stdout}\nstderr: ${error.stderr}`;
        }
        return `Command failed: ${error.message}`;
    }
}

function executeSearchFiles(pattern: string, searchPath?: string, filePattern?: string): string {
    const dir = searchPath || '.';
    let cmd = `grep -rn "${pattern}" "${dir}"`;

    if (filePattern) {
        cmd = `grep -rn --include="${filePattern}" "${pattern}" "${dir}"`;
    }

    try {
        const output = execSync(cmd, {
            encoding: 'utf-8',
            timeout: 30000,
            maxBuffer: 1024 * 1024,
        });

        const lines = output.trim().split('\n');
        if (lines.length > 50) {
            return lines.slice(0, 50).join('\n') + `\n... (${lines.length - 50} more matches)`;
        }
        return output || 'No matches found';
    } catch (error: any) {
        if (error.status === 1) {
            return 'No matches found';
        }
        return `Search failed: ${error.message}`;
    }
}

function executeEditFile(filePath: string, oldString: string, newString: string): string {
    const absolutePath = path.resolve(filePath);

    if (!fs.existsSync(absolutePath)) {
        return `File not found: ${absolutePath}`;
    }

    const content = fs.readFileSync(absolutePath, 'utf-8');

    if (!content.includes(oldString)) {
        return `String not found in file. Make sure to match exactly including whitespace.`;
    }

    const occurrences = content.split(oldString).length - 1;
    if (occurrences > 1) {
        return `Found ${occurrences} occurrences of the string. Please provide more context to match exactly one.`;
    }

    const newContent = content.replace(oldString, newString);
    fs.writeFileSync(absolutePath, newContent);

    return `File edited: ${absolutePath}`;
}

/**
 * Apply a unified diff patch to a file
 */
function executeApplyPatch(filePath: string, patch: string, fuzzFactor?: number): string {
    const absolutePath = path.resolve(filePath);

    if (!fs.existsSync(absolutePath)) {
        return `File not found: ${absolutePath}`;
    }

    const originalContent = fs.readFileSync(absolutePath, 'utf-8');

    // Normalize the patch - ensure it has proper headers if missing
    let normalizedPatch = patch;
    if (!patch.includes('---') && !patch.includes('+++')) {
        // Add minimal headers if they're missing
        normalizedPatch = `--- ${filePath}\n+++ ${filePath}\n${patch}`;
    }

    try {
        const result = applyPatch(originalContent, normalizedPatch, {
            fuzzFactor: fuzzFactor ?? 2,
        });

        if (result === false) {
            // Try parsing to give better error
            const parsed = parsePatch(normalizedPatch);
            if (parsed.length === 0) {
                return `Failed to apply patch: Invalid patch format`;
            }
            return `Failed to apply patch: Context lines don't match. Try increasing fuzzFactor or check the patch content.`;
        }

        fs.writeFileSync(absolutePath, result);
        return `Patch applied successfully to: ${absolutePath}`;
    } catch (error: any) {
        return `Failed to apply patch: ${error.message}`;
    }
}

/**
 * Create a unified diff patch from old and new content
 */
function executeCreatePatch(filePath: string, oldContent: string, newContent: string): string {
    try {
        const patch = createPatch(filePath, oldContent, newContent);
        return patch;
    } catch (error: any) {
        return `Failed to create patch: ${error.message}`;
    }
}

/**
 * Format tools for display
 */
export function formatToolList(): string {
    return builtinTools.map(t => {
        const f = t.function;
        const params = Object.keys(f.parameters.properties).join(', ');
        return `  ${f.name}(${params}) - ${f.description}`;
    }).join('\n');
}
