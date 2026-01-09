# Copilot API Bridge

A VS Code extension that exposes GitHub Copilot's language models as OpenAI-compatible APIs, enabling external tools like Codex CLI, Claude Code, or any OpenAI-compatible client to use your Copilot subscription.

## Features

- 🌉 **Multiple API Formats** - OpenAI Chat Completions, OpenAI Responses API, and Anthropic Messages API
- 🔄 **Streaming Support** - Full SSE streaming for all endpoints
- 🎯 **Extensive Model Mapping** - GPT-5.x, GPT-4.x, Claude 4.x, Gemini, O1 models
- ⚙️ **Configurable** - Port, auto-start, default model, CORS origins
- 📊 **Status Bar** - Visual indicator showing server status

## How It Works

```
┌─────────────────┐     HTTP      ┌──────────────────────┐     vscode.lm API     ┌─────────────────┐
│  Codex CLI /    │ ──────────▶   │  VS Code Extension   │ ─────────────────▶    │ GitHub Copilot  │
│  Claude Code /  │   localhost   │  (API Bridge Server) │                       │ (Claude, GPT,   │
│  Any Client     │               │                      │                       │  Gemini, O1...) │
└─────────────────┘               └──────────────────────┘                       └─────────────────┘
```

## Installation

### Development

1. Clone this repository
2. Run `npm install`
3. Run `npm run compile`
4. Press F5 to launch the extension in a new VS Code window

### From VSIX

1. Build with `npx vsce package`
2. Install the `.vsix` file in VS Code

## Usage

### Starting the Server

The server starts automatically when VS Code opens (configurable). You can also:

1. Open Command Palette (`Ctrl+Shift+P`)
2. Run **Copilot API Bridge: Start Server**

The status bar shows: `$(radio-tower) API Bridge :5168` when running.

### Commands

| Command | Description |
|---------|-------------|
| `Copilot API Bridge: Start Server` | Start the API server |
| `Copilot API Bridge: Stop Server` | Stop the API server |
| `Copilot API Bridge: Show Status` | Show current status |

### Configuring External Tools

```bash
# Set environment variables
export OPENAI_API_BASE=http://localhost:5168/v1
export OPENAI_API_KEY=dummy  # Not validated, but required by some tools
```

## API Endpoints

### Health Check

```bash
curl http://localhost:5168/
# or
curl http://localhost:5168/health
```

### GET /v1/models

List available models.

```bash
curl http://localhost:5168/v1/models
```

### POST /v1/chat/completions

OpenAI Chat Completions API.

```bash
curl http://localhost:5168/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-opus-4.5",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": false
  }'
```

### POST /v1/responses

OpenAI Responses API (used by Codex CLI).

```bash
curl http://localhost:5168/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5",
    "input": "Write hello world in Python",
    "stream": true
  }'
```

### POST /v1/messages

Anthropic Messages API.

```bash
curl http://localhost:5168/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-opus-4.5",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

## Supported Models

### Claude Models
- `claude-opus-4.5`, `claude-opus-4`, `claude-sonnet-4.5`, `claude-sonnet-4`
- `claude-haiku-4.5`, `claude-3.5-sonnet`

### GPT-5.x Models
- `gpt-5.2`, `gpt-5.1`, `gpt-5`, `gpt-5-mini`
- `gpt-5.1-codex`, `gpt-5.1-codex-max`, `gpt-5.1-codex-mini`, `gpt-5-codex`

### GPT-4.x Models
- `gpt-4.1`, `gpt-4o`, `gpt-4o-mini`, `gpt-4`, `gpt-4-turbo`, `gpt-3.5-turbo`

### Gemini Models
- `gemini-3-pro-preview`, `gemini-3-flash-preview`, `gemini-2.5-pro`

### O1 Models
- `o1`, `o1-preview`, `o1-mini`

### Special
- `auto` - Auto-select model
- `copilot-fast` - Fast Copilot model

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `copilot-api-bridge.port` | `5168` | Port for the API server |
| `copilot-api-bridge.autoStart` | `true` | Start server when VS Code opens |
| `copilot-api-bridge.defaultModel` | `claude-sonnet-4` | Default model when not specified |
| `copilot-api-bridge.allowedOrigins` | `["*"]` | CORS allowed origins |
| `copilot-api-bridge.bindAddress` | `0.0.0.0` | Bind address (use `127.0.0.1` for local-only) |

## Project Structure

```
bridge/
├── src/
│   ├── extension.ts      # Extension entry point
│   ├── server.ts         # HTTP server & routing
│   ├── statusBar.ts      # Status bar manager
│   ├── types.ts          # Types & model mapping
│   └── handlers/
│       ├── chatCompletions.ts  # /v1/chat/completions
│       ├── responses.ts        # /v1/responses
│       ├── messages.ts         # /v1/messages
│       └── models.ts           # /v1/models
├── scripts/              # Helper scripts
│   ├── start-claude.ps1  # Launch Claude Code with bridge (PowerShell)
│   ├── start-claude.sh   # Launch Claude Code with bridge (Bash/WSL)
│   ├── start-codex.ps1   # Launch Codex CLI with bridge (PowerShell)
│   └── start-codex.sh    # Launch Codex CLI with bridge (Bash/WSL)
├── tests/                # Test scripts
├── package.json
└── tsconfig.json
```

## Quick Start Scripts

Launch Claude Code or Codex CLI with environment variables pre-configured:

```powershell
# PowerShell (Windows)
.\scripts\start-claude.ps1
.\scripts\start-codex.ps1
```

```bash
# Bash (Linux/macOS)
./scripts/start-claude.sh
./scripts/start-codex.sh
```

## Limitations

- **VS Code must be running** - Extension runs inside VS Code
- **Rate limits apply** - Copilot subscription limits still apply
- **Limited API parity** - Not all OpenAI/Anthropic parameters are honored

## WSL Support

The extension binds to `0.0.0.0` by default, allowing connections from WSL. The helper scripts automatically detect WSL and use the correct Windows host IP:

```bash
# From WSL, run the helper script directly
./scripts/start-claude.sh
./scripts/start-codex.sh

# Or manually set the environment
export OPENAI_BASE_URL="http://$(ip route show default | awk '{print $3}'):5168/v1"
export OPENAI_API_KEY="dummy"
```

> **Note**: You may need to add a Windows Firewall rule to allow incoming connections on port 5168.

## Security

- Server binds to all interfaces (`0.0.0.0`) by default for WSL support
- Set `bindAddress` to `127.0.0.1` to restrict to localhost only
- No API key validation (relies on Copilot authentication)
- Respect GitHub Copilot's Terms of Service

## Development

```bash
# Install dependencies
npm install

# Compile
npm run compile

# Watch mode
npm run watch

# Lint
npm run lint
```

## License

MIT
