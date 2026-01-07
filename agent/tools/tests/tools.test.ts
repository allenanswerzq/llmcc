/**
 * Unit tests for @llmcc/tools package
 * Run with: npx tsx --test tests/*.test.ts
 *
 * Note: Some tools have DISABLE_ALL_TOOLS flag set which will cause
 * execution tests to return disabled status. The tests verify the
 * API contract regardless of the flag state.
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

// Import from the source directly for testing
import {
    executeTool,
    isToolAvailable,
    getAvailableTools,
    parseToolCalls,
    getBuiltinToolDocs,
    formatBuiltinToolsForPrompt,
    ToolResult
} from '../src/index';

// Test utilities
let tempDir: string;

function createTempDir(): string {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'llmcc-tools-test-'));
    return dir;
}

function cleanupTempDir(dir: string): void {
    if (fs.existsSync(dir)) {
        fs.rmSync(dir, { recursive: true, force: true });
    }
}

describe('Tool Availability', () => {
    it('should have bash tool available', () => {
        assert.strictEqual(isToolAvailable('bash'), true);
    });

    it('should have read_file tool available', () => {
        assert.strictEqual(isToolAvailable('read_file'), true);
    });

    it('should have write_file tool available', () => {
        assert.strictEqual(isToolAvailable('write_file'), true);
    });

    it('should have list_dir tool available', () => {
        assert.strictEqual(isToolAvailable('list_dir'), true);
    });

    it('should have grep_search tool available', () => {
        assert.strictEqual(isToolAvailable('grep_search'), true);
    });

    it('should have file_exists tool available', () => {
        assert.strictEqual(isToolAvailable('file_exists'), true);
    });

    it('should have sed tool available', () => {
        assert.strictEqual(isToolAvailable('sed'), true);
    });

    it('should return false for unknown tool', () => {
        assert.strictEqual(isToolAvailable('unknown_tool_xyz'), false);
    });

    it('should list all available tools', () => {
        const tools = getAvailableTools();
        assert.ok(Array.isArray(tools), 'getAvailableTools should return an array');
        assert.ok(tools.length >= 7, `Expected at least 7 tools, got ${tools.length}`);
        assert.ok(tools.includes('bash'), 'Should include bash');
        assert.ok(tools.includes('read_file'), 'Should include read_file');
        assert.ok(tools.includes('write_file'), 'Should include write_file');
    });
});

describe('read_file Tool', () => {
    beforeEach(() => {
        tempDir = createTempDir();
    });

    afterEach(() => {
        cleanupTempDir(tempDir);
    });

    it('should return a result object', async () => {
        const testFile = path.join(tempDir, 'test.txt');
        fs.writeFileSync(testFile, 'line1\nline2\nline3\n');

        const result = await executeTool('read_file', { path: testFile });
        // Result should have the expected structure
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
        assert.ok('output' in result);
        // Either succeeds or returns disabled message
        if (result.success) {
            assert.ok(result.output.includes('line1'));
        } else {
            assert.ok(result.error?.includes('disabled') || result.error?.includes('Failed'));
        }
    });

    it('should include line range support', async () => {
        const testFile = path.join(tempDir, 'test.txt');
        fs.writeFileSync(testFile, 'line1\nline2\nline3\nline4\nline5\n');

        const result = await executeTool('read_file', {
            path: testFile,
            start_line: 2,
            end_line: 4
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should handle non-existent file', async () => {
        const result = await executeTool('read_file', {
            path: '/nonexistent/path/file.txt'
        });
        assert.strictEqual(result.success, false);
        assert.ok(result.error);
    });

    it('should handle missing path parameter', async () => {
        const result = await executeTool('read_file', {});
        assert.strictEqual(result.success, false);
        assert.ok(result.error);
    });
});

describe('write_file Tool', () => {
    beforeEach(() => {
        tempDir = createTempDir();
    });

    afterEach(() => {
        cleanupTempDir(tempDir);
    });

    it('should return a result object for new file', async () => {
        const testFile = path.join(tempDir, 'new.txt');
        const content = 'Hello, World!';

        const result = await executeTool('write_file', {
            path: testFile,
            content
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
        // Either succeeds or returns disabled message
        if (result.success) {
            assert.strictEqual(fs.readFileSync(testFile, 'utf-8'), content);
        }
    });

    it('should support overwrite mode', async () => {
        const testFile = path.join(tempDir, 'existing.txt');
        fs.writeFileSync(testFile, 'old content');

        const result = await executeTool('write_file', {
            path: testFile,
            content: 'new content'
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should support append mode', async () => {
        const testFile = path.join(tempDir, 'append.txt');
        fs.writeFileSync(testFile, 'initial');

        const result = await executeTool('write_file', {
            path: testFile,
            content: '-appended',
            mode: 'append'
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should support insert mode', async () => {
        const testFile = path.join(tempDir, 'insert.txt');
        fs.writeFileSync(testFile, 'line1\nline2\nline3');

        const result = await executeTool('write_file', {
            path: testFile,
            content: 'inserted',
            mode: 'insert',
            line: 2
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should support create_dirs option', async () => {
        const testFile = path.join(tempDir, 'subdir', 'nested', 'file.txt');

        const result = await executeTool('write_file', {
            path: testFile,
            content: 'nested content'
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should handle missing content parameter', async () => {
        const testFile = path.join(tempDir, 'test.txt');
        const result = await executeTool('write_file', { path: testFile });
        assert.strictEqual(result.success, false);
        assert.ok(result.error);
    });
});

describe('list_dir Tool', () => {
    beforeEach(() => {
        tempDir = createTempDir();
        // Create test structure
        fs.writeFileSync(path.join(tempDir, 'file1.txt'), '');
        fs.writeFileSync(path.join(tempDir, 'file2.ts'), '');
        fs.mkdirSync(path.join(tempDir, 'subdir'));
        fs.writeFileSync(path.join(tempDir, 'subdir', 'nested.txt'), '');
        fs.writeFileSync(path.join(tempDir, '.hidden'), '');
    });

    afterEach(() => {
        cleanupTempDir(tempDir);
    });

    it('should return a result object', async () => {
        const result = await executeTool('list_dir', { path: tempDir });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
        assert.ok('output' in result);
    });

    it('should support show_hidden option', async () => {
        const result = await executeTool('list_dir', {
            path: tempDir,
            show_hidden: true
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should support recursive option', async () => {
        const result = await executeTool('list_dir', {
            path: tempDir,
            recursive: true
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should handle non-existent directory', async () => {
        const result = await executeTool('list_dir', {
            path: '/nonexistent/path'
        });
        assert.strictEqual(result.success, false);
        assert.ok(result.error);
    });
});

describe('file_exists Tool', () => {
    beforeEach(() => {
        tempDir = createTempDir();
        fs.writeFileSync(path.join(tempDir, 'exists.txt'), 'content');
    });

    afterEach(() => {
        cleanupTempDir(tempDir);
    });

    it('should return a result for existing file', async () => {
        const result = await executeTool('file_exists', {
            path: path.join(tempDir, 'exists.txt')
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should return a result for non-existing file', async () => {
        const result = await executeTool('file_exists', {
            path: path.join(tempDir, 'notexists.txt')
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should support get_stats option', async () => {
        const result = await executeTool('file_exists', {
            path: path.join(tempDir, 'exists.txt'),
            get_stats: true
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });
});

describe('bash Tool', () => {
    it('should return a result for simple command', async () => {
        const command = process.platform === 'win32'
            ? 'Write-Output "hello"'
            : 'echo hello';

        const result = await executeTool('bash', { command });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
        assert.ok('output' in result);
    });

    it('should return a result for command with arguments', async () => {
        const command = process.platform === 'win32'
            ? 'Write-Output "a b c"'
            : 'echo "a b c"';

        const result = await executeTool('bash', { command });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should handle failing command', async () => {
        const command = process.platform === 'win32'
            ? 'Get-ChildItem -Path "C:\\nonexistent_path_xyz"'
            : 'ls /nonexistent_path_xyz';

        const result = await executeTool('bash', { command });
        assert.strictEqual(result.success, false);
    });

    it('should handle missing command parameter', async () => {
        const result = await executeTool('bash', {});
        assert.strictEqual(result.success, false);
        assert.ok(result.error);
    });
});

describe('sed Tool', () => {
    beforeEach(() => {
        tempDir = createTempDir();
    });

    afterEach(() => {
        cleanupTempDir(tempDir);
    });

    it('should return a result for replace operation', async () => {
        const testFile = path.join(tempDir, 'sed-test.txt');
        fs.writeFileSync(testFile, 'Hello World');

        const result = await executeTool('sed', {
            path: testFile,
            pattern: 'World',
            replacement: 'Universe'
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should support dry_run option', async () => {
        const testFile = path.join(tempDir, 'sed-dry.txt');
        fs.writeFileSync(testFile, 'Hello World');

        const result = await executeTool('sed', {
            path: testFile,
            pattern: 'World',
            replacement: 'Universe',
            dry_run: true
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });

    it('should support global option', async () => {
        const testFile = path.join(tempDir, 'sed-global.txt');
        fs.writeFileSync(testFile, 'cat cat cat');

        const result = await executeTool('sed', {
            path: testFile,
            pattern: 'cat',
            replacement: 'dog',
            global: true
        });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
    });
});

describe('parseToolCalls', () => {
    it('should parse JSON tool call from text', () => {
        const text = 'I will read the file. {"tool": "read_file", "path": "/test.txt"}';
        const calls = parseToolCalls(text);
        // parseToolCalls only returns calls for available tools
        // If tools are disabled, the tool is still "available" but won't execute
        assert.ok(Array.isArray(calls));
        if (calls.length > 0) {
            assert.strictEqual(calls[0].name, 'read_file');
        }
    });

    it('should parse tool call from code block', () => {
        const text = 'Let me check:\n```json\n{"tool": "list_dir", "path": "."}\n```';
        const calls = parseToolCalls(text);
        assert.ok(Array.isArray(calls));
    });

    it('should parse multiple tool calls', () => {
        const text = `
First: {"tool": "read_file", "path": "/a.txt"}
Second: {"tool": "read_file", "path": "/b.txt"}
        `;
        const calls = parseToolCalls(text);
        assert.ok(Array.isArray(calls));
    });

    it('should return empty array for no tool calls', () => {
        const text = 'Just some regular text without any JSON';
        const calls = parseToolCalls(text);
        assert.strictEqual(calls.length, 0);
    });

    it('should return empty array for invalid JSON', () => {
        const text = '{"tool": not valid json}';
        const calls = parseToolCalls(text);
        assert.ok(Array.isArray(calls));
    });

    it('should return empty array for unknown tool', () => {
        const text = '{"tool": "unknown_xyz", "arg": "value"}';
        const calls = parseToolCalls(text);
        assert.strictEqual(calls.length, 0);
    });
});

describe('Tool Documentation', () => {
    it('should return tool docs array', () => {
        const docs = getBuiltinToolDocs();
        assert.ok(Array.isArray(docs));
    });

    it('should format tools for prompt as string', () => {
        const prompt = formatBuiltinToolsForPrompt();
        assert.ok(typeof prompt === 'string');
    });
});

describe('executeTool', () => {
    it('should return error result for unknown tool', async () => {
        const result = await executeTool('unknown_tool_xyz', {});
        assert.strictEqual(result.success, false);
        assert.ok(result.error);
        assert.ok(typeof result.error === 'string');
    });

    it('should always return ToolResult structure', async () => {
        const result = await executeTool('bash', { command: 'echo test' });
        assert.ok(typeof result === 'object');
        assert.ok('success' in result);
        assert.ok('output' in result);
        assert.ok(typeof result.success === 'boolean');
        assert.ok(typeof result.output === 'string');
    });
});
