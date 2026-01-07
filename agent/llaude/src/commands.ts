/**
 * Command handlers for llaude REPL
 */

import * as fs from 'fs';
import * as path from 'path';
import { Session, Config, CommandResult } from './types.js';
import { formatToolList } from './tools.js';

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

// Model aliases for convenience
const MODEL_ALIASES: Record<string, string> = {
    'sonnet': 'claude-sonnet-4-20250514',
    'opus': 'claude-opus-4-20250514',
    'haiku': 'claude-haiku-4-20250514',
    's4': 'claude-sonnet-4-20250514',
    'o4': 'claude-opus-4-20250514',
    'h4': 'claude-haiku-4-20250514',
    'gpt4': 'gpt-4',
    'gpt4o': 'gpt-4o',
    'gpt4turbo': 'gpt-4-turbo',
    'o1': 'o1',
    'o1-mini': 'o1-mini',
    'o3': 'o3',
    'o3-mini': 'o3-mini',
};

function resolveModelAlias(model: string): string {
    return MODEL_ALIASES[model.toLowerCase()] || model;
}

export function executeCommand(
    input: string,
    session: Session,
    config: Config
): CommandResult {
    const parts = input.trim().split(/\s+/);
    const cmd = parts[0].toLowerCase();
    const args = parts.slice(1);

    switch (cmd) {
        case '/help':
        case '/h':
        case '/?':
            return {
                session,
                config,
                shouldExit: false,
                output: `
${colors.cyan}${colors.bold}llaude commands:${colors.reset}

  ${colors.green}/help${colors.reset}           Show this help
  ${colors.green}/clear${colors.reset}          Clear conversation history
  ${colors.green}/history${colors.reset}        Show conversation history
  ${colors.green}/save${colors.reset}           Force save session
  ${colors.green}/config${colors.reset}         Show current config
  ${colors.green}/model <name>${colors.reset}   Change model (aliases: sonnet, opus, haiku, gpt4o)
  ${colors.green}/models${colors.reset}         Show available model aliases
  ${colors.green}/tools${colors.reset}          Show available tools
  ${colors.green}/system [text]${colors.reset}  Show/set system prompt
  ${colors.green}/tokens${colors.reset}         Show token count estimate
  ${colors.green}/export <file>${colors.reset}  Export conversation to file
  ${colors.green}/exit${colors.reset}           Exit llaude

${colors.dim}Multi-line: End line with \\ to continue${colors.reset}
`,
            };

        case '/exit':
        case '/quit':
        case '/q':
            return {
                session,
                config,
                shouldExit: true,
            };

        case '/clear':
            return {
                session: {
                    ...session,
                    messages: [],
                    id: Date.now().toString(),
                    created: new Date().toISOString(),
                },
                config,
                shouldExit: false,
                output: `${colors.dim}Conversation cleared${colors.reset}`,
            };

        case '/history':
            if (session.messages.length === 0) {
                return {
                    session,
                    config,
                    shouldExit: false,
                    output: `${colors.dim}No messages in history${colors.reset}`,
                };
            }

            let historyOutput = `${colors.cyan}${colors.bold}Conversation history (${session.messages.length} messages):${colors.reset}\n\n`;

            for (let i = 0; i < session.messages.length; i++) {
                const msg = session.messages[i];
                const role = msg.role === 'user'
                    ? `${colors.green}You`
                    : `${colors.blue}Assistant`;
                const time = new Date(msg.timestamp).toLocaleTimeString();
                const preview = msg.content.length > 80
                    ? msg.content.slice(0, 80) + '...'
                    : msg.content;
                historyOutput += `${colors.dim}[${i + 1}] ${time}${colors.reset} ${role}${colors.reset}: ${preview}\n`;
            }

            return {
                session,
                config,
                shouldExit: false,
                output: historyOutput,
            };

        case '/save':
            return {
                session,
                config,
                shouldExit: false,
                output: `${colors.dim}Session saved${colors.reset}`,
            };

        case '/config':
            return {
                session,
                config,
                shouldExit: false,
                output: `${colors.cyan}${colors.bold}Current configuration:${colors.reset}

  ${colors.green}Model:${colors.reset}      ${config.model}
  ${colors.green}API URL:${colors.reset}    ${config.baseUrl}
  ${colors.green}Max tokens:${colors.reset} ${config.maxTokens}
  ${colors.green}System:${colors.reset}     ${config.systemPrompt.slice(0, 60)}...
`,
            };

        case '/model':
            if (args.length === 0) {
                return {
                    session,
                    config,
                    shouldExit: false,
                    output: `${colors.dim}Current model: ${config.model}\nUsage: /model <name> (e.g., sonnet, opus, gpt4o)${colors.reset}`,
                };
            }
            const resolvedModel = resolveModelAlias(args[0]);
            const wasAlias = resolvedModel !== args[0];
            return {
                session,
                config: { ...config, model: resolvedModel },
                shouldExit: false,
                output: wasAlias
                    ? `${colors.dim}Model changed to: ${resolvedModel} (from alias: ${args[0]})${colors.reset}`
                    : `${colors.dim}Model changed to: ${resolvedModel}${colors.reset}`,
            };

        case '/models':
            const modelList = Object.entries(MODEL_ALIASES)
                .map(([alias, model]) => `  ${colors.green}${alias.padEnd(12)}${colors.reset} → ${model}`)
                .join('\n');
            return {
                session,
                config,
                shouldExit: false,
                output: `${colors.cyan}${colors.bold}Model aliases:${colors.reset}\n${modelList}\n\n${colors.dim}Current: ${config.model}${colors.reset}`,
            };

        case '/tools':
            return {
                session,
                config,
                shouldExit: false,
                output: `${colors.cyan}${colors.bold}Available tools:${colors.reset}\n\n${formatToolList()}\n\n${colors.dim}Tools are automatically used by the model when needed.${colors.reset}`,
            };

        case '/system':
            if (args.length === 0) {
                return {
                    session,
                    config,
                    shouldExit: false,
                    output: `${colors.cyan}System prompt:${colors.reset}\n${config.systemPrompt}`,
                };
            }
            const newPrompt = args.join(' ');
            return {
                session,
                config: { ...config, systemPrompt: newPrompt },
                shouldExit: false,
                output: `${colors.dim}System prompt updated${colors.reset}`,
            };

        case '/tokens':
            // Rough token estimate (4 chars per token)
            const totalChars = session.messages.reduce((sum, m) => sum + m.content.length, 0);
            const estimatedTokens = Math.ceil(totalChars / 4);
            return {
                session,
                config,
                shouldExit: false,
                output: `${colors.dim}Estimated tokens in context: ~${estimatedTokens} (${session.messages.length} messages, ${totalChars} chars)${colors.reset}`,
            };

        case '/export':
            if (args.length === 0) {
                return {
                    session,
                    config,
                    shouldExit: false,
                    output: `${colors.yellow}Usage: /export <filename>${colors.reset}`,
                };
            }

            const exportPath = args[0];
            let exportContent = `# llaude conversation\n`;
            exportContent += `# Session: ${session.id}\n`;
            exportContent += `# Created: ${session.created}\n\n`;

            for (const msg of session.messages) {
                const role = msg.role === 'user' ? 'You' : 'Assistant';
                exportContent += `## ${role}\n\n${msg.content}\n\n---\n\n`;
            }

            try {
                fs.writeFileSync(exportPath, exportContent);
                return {
                    session,
                    config,
                    shouldExit: false,
                    output: `${colors.dim}Exported to: ${exportPath}${colors.reset}`,
                };
            } catch (error) {
                return {
                    session,
                    config,
                    shouldExit: false,
                    output: `${colors.red}Export failed: ${error instanceof Error ? error.message : error}${colors.reset}`,
                };
            }

        default:
            return {
                session,
                config,
                shouldExit: false,
                output: `${colors.yellow}Unknown command: ${cmd}. Type /help for available commands.${colors.reset}`,
            };
    }
}
