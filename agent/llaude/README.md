# llaude

A simple persistent REPL code agent backed by JSONC, with tool calling support.

## Installation

```bash
cd agent/llaude
npm install
npm run build
npm link  # Makes 'llaude' available globally
```

## Usage

```bash
# Start fresh session
llaude --new

# Continue previous session
llaude

# Pipe input
echo "Explain async/await" | llaude
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
| `/system [text]` | Show/set system prompt |
| `/tokens` | Show token count estimate |
| `/export <file>` | Export conversation to file |
| `/exit` | Exit llaude |

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

llaude can use these tools automatically when needed:

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

Example:
```
> list the files in this directory
[calling list_dir...]
Here are the files: ...

> read the first 20 lines of package.json
[calling read_file...]

> apply this patch to fix the bug:
  @@ -10,3 +10,4 @@
   existing line
  -old line
  +new fixed line
[calling apply_patch...]
Patch applied successfully!
```

## Multi-line Input

End a line with `\` to continue on the next line:

```
> Write a function that \
... calculates fibonacci \
... numbers recursively
```

## Configuration

Config is stored in `~/.llaude/config.jsonc`:

```jsonc
{
  // API settings (defaults to Copilot bridge)
  "apiKey": "copilot-bridge-key",
  "baseUrl": "http://localhost:5168",

  // Model to use
  "model": "claude-sonnet-4",
  "maxTokens": 4096,

  // System prompt
  "systemPrompt": "You are llaude, a helpful coding assistant..."
}
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `LLAUDE_CONFIG_DIR` | Override config directory (default: `~/.llaude`) |
| `ANTHROPIC_API_KEY` | API key for Anthropic API |
| `ANTHROPIC_BASE_URL` | API base URL |

## Session Storage

Session history is stored in `~/.llaude/session.jsonc` and persists across restarts.
