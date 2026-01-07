#!/usr/bin/env node
/**
 * llaude - Simple persistent code agent REPL
 *
 * A lightweight REPL that maintains conversation history in a JSONC file.
 * No fancy TUI - just stdin/stdout with persistent state.
 */

import * as readline from 'readline';
import * as fs from 'fs';
import * as path from 'path';
import { Session, Message, Config } from './types.js';
import { callAPI } from './api.js';
import { executeCommand } from './commands.js';

// Default paths
const CONFIG_DIR = process.env.LLAUDE_CONFIG_DIR || path.join(process.env.HOME || '', '.llaude');
const SESSION_FILE = path.join(CONFIG_DIR, 'session.jsonc');
const CONFIG_FILE = path.join(CONFIG_DIR, 'config.jsonc');

// Colors for terminal output
const colors = {
    reset: '\x1b[0m',
    bold: '\x1b[1m',
    dim: '\x1b[2m',
    green: '\x1b[32m',
    blue: '\x1b[34m',
    cyan: '\x1b[36m',
    yellow: '\x1b[33m',
    red: '\x1b[31m',
    magenta: '\x1b[35m',
};

function ensureConfigDir(): void {
    if (!fs.existsSync(CONFIG_DIR)) {
        fs.mkdirSync(CONFIG_DIR, { recursive: true });
    }
}

function loadConfig(): Config {
    ensureConfigDir();

    const defaultConfig: Config = {
        apiKey: process.env.ANTHROPIC_API_KEY || 'copilot-bridge-key',
        baseUrl: process.env.ANTHROPIC_BASE_URL || 'http://localhost:5168',
        model: 'claude-sonnet-4',
        maxTokens: 4096,
        systemPrompt: `You are llaude, a helpful coding assistant. You help with code, answer questions, and assist with development tasks. Be concise but thorough.`,
    };

    if (fs.existsSync(CONFIG_FILE)) {
        try {
            const content = fs.readFileSync(CONFIG_FILE, 'utf-8');
            // Strip JSONC comments (single-line // and multi-line /* */)
            const json = content
                .split('\n')
                .map(line => {
                    // Remove // comments (but not inside strings)
                    const commentIdx = line.indexOf('//');
                    if (commentIdx !== -1) {
                        // Check if it's inside a string by counting quotes before it
                        const beforeComment = line.slice(0, commentIdx);
                        const quoteCount = (beforeComment.match(/"/g) || []).length;
                        if (quoteCount % 2 === 0) {
                            return line.slice(0, commentIdx);
                        }
                    }
                    return line;
                })
                .join('\n')
                .replace(/\/\*[\s\S]*?\*\//g, '');
            const loaded = JSON.parse(json);
            return { ...defaultConfig, ...loaded };
        } catch (e) {
            // Silent fallback to defaults
        }
    } else {
        // Write default config
        const configContent = `// llaude configuration
{
  // API settings (defaults to Copilot bridge)
  "apiKey": "${defaultConfig.apiKey}",
  "baseUrl": "${defaultConfig.baseUrl}",

  // Model to use
  "model": "${defaultConfig.model}",
  "maxTokens": ${defaultConfig.maxTokens},

  // System prompt
  "systemPrompt": "${defaultConfig.systemPrompt}"
}
`;
        fs.writeFileSync(CONFIG_FILE, configContent);
    }

    return defaultConfig;
}

function loadSession(): Session {
    ensureConfigDir();

    const defaultSession: Session = {
        id: Date.now().toString(),
        created: new Date().toISOString(),
        messages: [],
    };

    if (fs.existsSync(SESSION_FILE)) {
        try {
            const content = fs.readFileSync(SESSION_FILE, 'utf-8');
            // Strip JSONC comments
            const json = content.replace(/\/\/.*$/gm, '').replace(/\/\*[\s\S]*?\*\//g, '');
            return JSON.parse(json);
        } catch (e) {
            console.error(`${colors.yellow}Warning: Could not parse session, starting fresh${colors.reset}`);
        }
    }

    return defaultSession;
}

function saveSession(session: Session): void {
    ensureConfigDir();

    const content = `// llaude session - ${session.created}
// Session ID: ${session.id}
// Messages: ${session.messages.length}
${JSON.stringify(session, null, 2)}
`;
    fs.writeFileSync(SESSION_FILE, content);
}

function printWelcome(session: Session, config: Config): void {
    console.log(`${colors.cyan}${colors.bold}llaude${colors.reset} ${colors.dim}v0.1.0${colors.reset}`);
    console.log(`${colors.dim}Model: ${config.model} | API: ${config.baseUrl}${colors.reset}`);

    if (session.messages.length > 0) {
        console.log(`${colors.dim}Restored session with ${session.messages.length} messages${colors.reset}`);
    }

    console.log(`${colors.dim}Commands: /help /clear /history /save /exit${colors.reset}`);
    console.log();
}

function printHelp(): void {
    console.log(`
${colors.cyan}${colors.bold}llaude commands:${colors.reset}

  ${colors.green}/help${colors.reset}      Show this help
  ${colors.green}/clear${colors.reset}     Clear conversation history
  ${colors.green}/history${colors.reset}   Show conversation history
  ${colors.green}/save${colors.reset}      Force save session
  ${colors.green}/config${colors.reset}    Show current config
  ${colors.green}/model${colors.reset}     Change model (e.g., /model claude-opus-4)
  ${colors.green}/system${colors.reset}    Show/set system prompt
  ${colors.green}/exit${colors.reset}      Exit llaude

${colors.dim}Multi-line input: End with \\ to continue on next line${colors.reset}
`);
}

function printHistory(session: Session): void {
    if (session.messages.length === 0) {
        console.log(`${colors.dim}No messages in history${colors.reset}`);
        return;
    }

    console.log(`${colors.cyan}${colors.bold}Conversation history:${colors.reset}\n`);

    for (const msg of session.messages) {
        const role = msg.role === 'user' ? `${colors.green}You` : `${colors.blue}Assistant`;
        const time = new Date(msg.timestamp).toLocaleTimeString();
        const preview = msg.content.slice(0, 100) + (msg.content.length > 100 ? '...' : '');
        console.log(`${colors.dim}[${time}]${colors.reset} ${role}${colors.reset}: ${preview}`);
    }
    console.log();
}

async function handleInput(
    input: string,
    session: Session,
    config: Config
): Promise<{ session: Session; config: Config; shouldExit: boolean }> {
    const trimmed = input.trim();

    // Handle commands
    if (trimmed.startsWith('/')) {
        const result = executeCommand(trimmed, session, config);
        if (result.output) {
            console.log(result.output);
        }
        return {
            session: result.session,
            config: result.config,
            shouldExit: result.shouldExit
        };
    }

    if (!trimmed) {
        return { session, config, shouldExit: false };
    }

    // Add user message
    const userMessage: Message = {
        role: 'user',
        content: trimmed,
        timestamp: new Date().toISOString(),
    };
    session.messages.push(userMessage);

    // Call API
    console.log();
    process.stdout.write(`${colors.blue}${colors.bold}llaude${colors.reset}: `);

    try {
        // Callback to show tool calls in real-time
        const onToolCall = (name: string, args: string) => {
            console.log(`\n${colors.dim}[calling ${name}...]${colors.reset}`);
        };

        const response = await callAPI(session.messages, config, onToolCall);

        // Add assistant message
        const assistantMessage: Message = {
            role: 'assistant',
            content: response,
            timestamp: new Date().toISOString(),
        };
        session.messages.push(assistantMessage);

        console.log(response);
        console.log();

        // Auto-save
        saveSession(session);

    } catch (error) {
        console.log(`${colors.red}Error: ${error instanceof Error ? error.message : error}${colors.reset}`);
        // Remove failed user message
        session.messages.pop();
    }

    return { session, config, shouldExit: false };
}

async function main(): Promise<void> {
    let config = loadConfig();
    let session = loadSession();

    // Parse args
    const args = process.argv.slice(2);
    if (args.includes('--new') || args.includes('-n')) {
        session = {
            id: Date.now().toString(),
            created: new Date().toISOString(),
            messages: [],
        };
    }

    printWelcome(session, config);

    const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
        prompt: `${colors.green}>${colors.reset} `,
    });

    let multiLineBuffer = '';
    let pendingOperation: Promise<void> | null = null;

    rl.prompt();

    rl.on('line', (line) => {
        // Handle multi-line input
        if (line.endsWith('\\')) {
            multiLineBuffer += line.slice(0, -1) + '\n';
            process.stdout.write(`${colors.dim}...${colors.reset} `);
            return;
        }

        const input = multiLineBuffer + line;
        multiLineBuffer = '';

        // Track the async operation
        pendingOperation = (async () => {
            const result = await handleInput(input, session, config);
            session = result.session;
            config = result.config;

            if (result.shouldExit) {
                saveSession(session);
                console.log(`${colors.dim}Session saved. Goodbye!${colors.reset}`);
                rl.close();
                process.exit(0);
            }

            rl.prompt();
        })();
    });

    rl.on('close', async () => {
        // Wait for any pending operation to complete
        if (pendingOperation) {
            await pendingOperation;
        }
        saveSession(session);
        console.log(`\n${colors.dim}Session saved. Goodbye!${colors.reset}`);
        process.exit(0);
    });

    // Handle Ctrl+C gracefully
    process.on('SIGINT', () => {
        saveSession(session);
        console.log(`\n${colors.dim}Session saved. Goodbye!${colors.reset}`);
        process.exit(0);
    });
}

main().catch((error) => {
    console.error(`Fatal error: ${error.message}`);
    process.exit(1);
});
