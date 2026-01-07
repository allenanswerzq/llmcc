# Browser Bridge

Native messaging bridge for Claude Code browser automation. Replaces the Claude in Chrome extension entirely, allowing `claude --chrome` to work without any browser extension.

## Architecture

```
┌─────────────────┐     Native Messaging          ┌─────────────────┐
│   Claude Code   │ ─────────────────────────────▶│  Browser Bridge │
│   (--chrome)    │   4-byte len + JSON           │   (this pkg)    │
└─────────────────┘                               └────────┬────────┘
                                                           │
                                                           │ Puppeteer
                                                           ▼
                                                  ┌─────────────────┐
                                                  │     Browser     │
                                                  │ (Chrome/Firefox)│
                                                  └─────────────────┘
```

## Quick Start

```bash
# Install dependencies
cd agent/browser-bridge
npm install

# Build
npm run build

# Register native messaging host
npm run install-host      # Linux/macOS
npm run install-host:win  # Windows (PowerShell)

# Test the bridge
npm test

# Use with Claude Code
claude --chrome -p "Navigate to example.com and take a screenshot"
```

## Structure

```
browser-bridge/
├── src/
│   ├── index.ts              # Entry point - Native messaging host
│   ├── native-messaging.ts   # Protocol handler (4-byte length + JSON)
│   ├── browser-controller.ts # Puppeteer wrapper
│   └── mcp-server.ts         # MCP tool dispatcher
├── scripts/
│   ├── install-host.sh       # Register native messaging host (Linux/Mac)
│   ├── install-host.ps1      # Register native messaging host (Windows)
│   └── test-protocol.js      # Test the native messaging protocol
└── dist/                     # Compiled output
```

## How It Works

1. **Claude Code** spawns the bridge as a child process when `--chrome` is used
2. **Native Messaging** protocol: 4-byte little-endian length prefix + UTF-8 JSON
3. **MCP Protocol** for tool calls: `initialize`, `tools/list`, `tools/call`
4. **Puppeteer** controls a headless Chrome browser

## Implemented Tools

| Tool | Description |
|------|-------------|
| `navigate` | Navigate to URL |
| `read_page` | Get accessibility tree (depth configurable) |
| `find` | Find interactive elements by text query |
| `get_page_text` | Extract page text content |
| `computer` | Mouse/keyboard actions and screenshots |
| `tabs_create` | Create new browser tab |
| `tabs_context` | Get list of open tabs |
| `form_input` | Fill form fields by CSS selector |
| `javascript_tool` | Execute JavaScript in page context |
| `read_console_messages` | Get browser console logs |
| `read_network_requests` | Get network request log |
| `resize_window` | Resize browser viewport |

## Computer Tool Actions

The `computer` tool supports these actions:
- `screenshot` - Take a screenshot (returns base64 image)
- `click` - Click at coordinates
- `double_click` - Double-click at coordinates
- `right_click` - Right-click at coordinates
- `type` - Type text at current focus
- `key` - Press a key (e.g., "Enter", "Tab", "Escape")
- `scroll` - Scroll up/down/left/right
- `move` - Move mouse to coordinates

## Development

```bash
# Watch mode (rebuild on changes)
npm run watch

# Run with visible browser for debugging
npm run start:visible

# Lint
npm run lint
```

## Troubleshooting

### "Extension: Not detected" in Claude Code

The native messaging host isn't registered properly. Re-run:
```bash
npm run install-host
```

### Browser doesn't open

The bridge uses headless mode by default. For debugging:
```bash
npm run start:visible
```

### WSL Issues

On WSL, you need to install the host on the Linux side. The bridge runs in Linux and controls a headless Chrome in Linux (not Windows Chrome).
