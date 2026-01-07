# llcraft

A persistent code agent REPL with browser automation support. Uses the Copilot API bridge for LLM access and the chrome package for browser control.

## Features

- 🔄 **Persistent Sessions** - Conversation history saved across restarts
- 🛠️ **Tool Calling** - File operations, shell commands, code editing
- 🌐 **Browser Automation** - Control Chrome via the chrome package
- 🎯 **Model Switching** - Quick aliases for common models
- 📝 **JSONC Config** - Human-readable configuration with comments

## Installation

```bash
cd agent/llcraft
npm install
npm run build
npm link  # Makes 'llcraft' available globally
```

## Usage

```bash
# Start fresh session
llcraft --new

# Continue previous session
llcraft

# Enable browser automation
llcraft --chrome

# Pipe input
echo "Explain async/await" | llcraft
```

## Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help |
| `/clear` | Clear conversation history |
| `/history` | Show conversation history |
| `/save` | Force save session |
| `/config` | Show current config |
| `/model <name>` | Change model (supports aliases) |
| `/models` | Show available model aliases |
| `/tools` | Show available tools |
| `/chrome` | Toggle browser automation |
| `/system [text]` | Show/set system prompt |
| `/tokens` | Show token count estimate |
| `/export <file>` | Export conversation to file |
| `/exit` | Exit llcraft |

## Model Aliases

Quick model switching with `/model <alias>`:

| Alias | Model |
|-------|-------|
| `sonnet`, `s4` | claude-sonnet-4-20250514 |
| `opus`, `o4` | claude-opus-4-20250514 |
| `haiku`, `h4` | claude-haiku-4-20250514 |
| `gpt4o` | gpt-4o |
| `o1`, `o3` | o1, o3 |

## Tools

### Code Tools
llcraft can use these tools automatically when needed:

| Tool | Description |
|------|-------------|
| `read_file` | Read file contents (with optional line ranges) |
| `write_file` | Write/create files |
| `list_dir` | List directory contents |
| `run_command` | Execute shell commands |
| `search_files` | Grep for patterns |
| `edit_file` | Replace text in files |
| `apply_patch` | Apply unified diff patches (via [jsdiff](https://github.com/kpdecker/jsdiff)) |
| `create_patch` | Generate unified diff from old/new content |

### Browser Tools (with `--chrome`)
When browser automation is enabled, these additional tools are available:

| Tool | Description |
|------|-------------|
| `navigate` | Navigate to a URL |
| `read_page` | Get accessibility tree |
| `find` | Find elements by text query |
| `get_page_text` | Extract page text content |
| `computer` | Mouse/keyboard/screenshot |
| `tabs_create` | Create new browser tab |
| `tabs_context` | Get list of open tabs |
| `form_input` | Fill form fields |
| `javascript_tool` | Execute JavaScript in page |

Example:
```
> list the files in this directory
[calling list_dir...]
Here are the files: ...

> read the first 20 lines of package.json
[calling read_file...]

> navigate to github.com and take a screenshot
[calling navigate...]
[calling computer with action=screenshot...]
Here's the screenshot: ...
```

## Multi-line Input

End a line with `\` to continue on the next line:

```
> Write a function that \
... calculates fibonacci \
... numbers recursively
```

## Configuration

Config is stored in `~/.llcraft/config.jsonc`:

```jsonc
{
  // API settings (defaults to Copilot bridge)
  "apiKey": "copilot-bridge-key",
  "baseUrl": "http://localhost:5168",

  // Model to use
  "model": "claude-opus-4.5",
  "maxTokens": 4096,

  // Browser automation (requires chrome package installed)
  "chrome": false,

  // System prompt
  "systemPrompt": "You are llcraft, a helpful coding assistant..."
}
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `LLCRAFT_CONFIG_DIR` | Override config directory (default: `~/.llcraft`) |
| `ANTHROPIC_API_KEY` | API key for Anthropic API |
| `ANTHROPIC_BASE_URL` | API base URL |

## Session Storage

Session history is stored in `~/.llcraft/session.jsonc` and persists across restarts.
