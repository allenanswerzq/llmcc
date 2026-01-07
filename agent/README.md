# LLMCC Agent Workspace

This workspace contains agent-related packages for llmcc - tools and infrastructure for running AI coding agents with GitHub Copilot as the LLM backend.

## Overview

The main goal is to let you use external AI tools (Claude Code, Codex CLI, or any OpenAI-compatible client) with your GitHub Copilot subscription instead of paying for separate API access.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              How It All Fits Together                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─────────────┐      ┌─────────────┐      ┌─────────────┐                     │
│   │ Claude Code │      │  Codex CLI  │      │    llcraft  │   External Clients  │
│   └──────┬──────┘      └──────┬──────┘      └──────┬──────┘                     │
│          │                    │                    │                            │
│          └────────────────────┼────────────────────┘                            │
│                               │ HTTP (OpenAI/Anthropic API)                     │
│                               ▼                                                 │
│   ┌─────────────────────────────────────────────────────────────────┐           │
│   │                        bridge (VS Code Extension)               │           │
│   │  - Translates API calls to VS Code Language Model API           │           │
│   │  - Supports streaming, tool calls, multiple API formats         │           │
│   └─────────────────────────────────────────┬───────────────────────┘           │
│                                             │ vscode.lm API                     │
│                                             ▼                                   │
│   ┌─────────────────────────────────────────────────────────────────┐           │
│   │                      GitHub Copilot                             │           │
│   │  Claude, GPT-5.x, GPT-4.x, Gemini, O1 models                    │           │
│   └─────────────────────────────────────────────────────────────────┘           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Packages

### [bridge](./bridge/) - Copilot API Bridge

A VS Code extension that exposes GitHub Copilot's language models as OpenAI-compatible APIs.

**Use this when:** You want to use Claude Code, Codex CLI, or any OpenAI/Anthropic-compatible tool with your Copilot subscription.

```bash
# Quick start
cd bridge
npm install && npm run compile
# Press F5 in VS Code to launch

# Then configure your tool
export OPENAI_API_BASE=http://localhost:5168/v1
export OPENAI_API_KEY=dummy
```

**Supported APIs:**
- OpenAI Chat Completions (`/v1/chat/completions`)
- OpenAI Responses API (`/v1/responses`) - used by Codex CLI
- Anthropic Messages API (`/v1/messages`)

### [chrome](./chrome/) - Browser Automation Bridge

Native messaging host for browser automation via MCP (Model Context Protocol).

**Use this when:** You want AI agents to interact with web pages (scraping, testing, automation).

**Works with:**
- ✅ Claude Code (`claude --chrome`)
- ✅ llcraft (`llcraft --chrome`)

```bash
# Install the native messaging host
cd chrome
npm install && npm run build
npm run install-host      # Linux/macOS
npm run install-host:win  # Windows

# Use with Claude Code
claude --chrome -p "Navigate to example.com and take a screenshot"

# Use with llcraft
llcraft --chrome
```

**Browser tools available:** `navigate`, `read_page`, `find`, `get_page_text`, `computer` (mouse/keyboard/screenshot), `tabs_create`, `tabs_context`, `form_input`, `javascript_tool`, and more.

### [llcraft](./llcraft/) - Code Agent REPL

A persistent code agent REPL with browser automation support. Combines file/shell tools with browser control.

**Use this when:** You want a terminal AI assistant that can edit code AND control the browser.

```bash
cd llcraft
npm install && npm run build
npm link  # Makes 'llcraft' available globally

# Start session
llcraft

# With browser automation
llcraft --chrome
```

### [tools](./tools/) - Built-in Tool Implementations

Shared tool implementations used by llmcc agents. Provides filesystem operations, shell execution, and code analysis.

**Available tools:**
| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands |
| `read_file` | Read file contents with line ranges |
| `write_file` | Write or create files |
| `list_dir` | List directory contents |
| `file_exists` | Check if file/directory exists |
| `grep_search` | Search for patterns in files |
| `sed` | Stream editor for text transformation |
| `llmcc` | Code architecture graph generator |

## Quick Start

### 1. Use Codex CLI with Copilot

```powershell
# PowerShell (Windows)
cd agent
.\scripts\start-codex.ps1
```

```bash
# Bash (Linux/macOS/WSL)
cd agent
./scripts/start-codex.sh
```

### 2. Use Claude Code with Copilot

```powershell
# PowerShell (Windows)
cd agent
.\scripts\start-claude.ps1
```

```bash
# Bash (Linux/macOS/WSL)
cd agent
./scripts/start-claude.sh
```

## Development

```bash
# Install all dependencies
npm install

# Build all packages
npm run build

# Build individual packages
npm run build:bridge
npm run build:chrome
npm run build:llcraft
npm run build:tools

# Watch mode for bridge development
cd bridge && npm run watch
```

## Testing

```bash
# Run all tests
npm run test

# Run tests for specific packages
npm run test:tools      # Unit tests for tools package (43 tests)
npm run test:bridge     # Unit tests for bridge package (20 tests)

# Run integration tests (requires bridge server running)
npm run test:integration
```

### Test Structure

```
agent/
├── tools/tests/              # Tool implementation tests
│   └── tools.test.ts         # 43 unit tests
├── bridge/tests/
│   ├── unit/types.test.ts    # 20 unit tests for types & routing
│   └── integration.test.ts   # Integration tests (require server)
```

## Workspace Structure

```
agent/
├── package.json          # Workspace root (npm workspaces)
├── README.md             # This file
├── scripts/              # Quick-start scripts for Claude/Codex
│   ├── start-claude.ps1
│   ├── start-claude.sh
│   ├── start-codex.ps1
│   └── start-codex.sh
├── bridge/               # VS Code extension
│   ├── src/
│   │   ├── extension.ts  # Extension entry point
│   │   ├── server.ts     # HTTP server & routing
│   │   ├── types.ts      # Model mapping & types
│   │   └── handlers/     # API endpoint handlers
│   └── tests/
├── chrome/               # Browser automation
│   ├── src/
│   └── scripts/
├── llcraft/              # Code agent REPL with browser support
│   └── src/
└── tools/                # Shared tool implementations
    ├── src/index.ts      # All tool implementations
    └── tests/
```

## Requirements

- **VS Code** with GitHub Copilot extension (for bridge)
- **Node.js** 18+
- **GitHub Copilot subscription** (Individual, Business, or Enterprise)

## Troubleshooting

### Bridge server not starting
1. Ensure GitHub Copilot extension is installed and authenticated
2. Check VS Code Output panel → "Copilot API Bridge"
3. Verify port 5168 is not in use

### WSL connection issues
1. Bridge binds to `0.0.0.0` by default for WSL access
2. May need Windows Firewall rule for port 5168
3. Scripts auto-detect Windows host IP in WSL

### Model not available
- Not all models are available in all Copilot tiers
- Check Copilot settings for available models
- Try `claude-sonnet-4` or `gpt-4o` as fallback
