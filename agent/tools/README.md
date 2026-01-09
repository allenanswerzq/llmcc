# @llmcc/tools

Built-in tool implementations for llmcc agents.

## Tools

- **bash** - Execute shell commands
- **read_file** - Read file contents with line range support
- **write_file** - Write/append/insert content to files
- **list_dir** - List directory contents with recursion and filtering
- **grep_search** - Search for text patterns in files
- **file_exists** - Check if files/directories exist
- **sed** - Find and replace text using regex
- **llmcc** - Generate architecture graphs for code understanding

## Usage

```typescript
import { executeBuiltinTool, isBuiltinTool, getBuiltinToolDocs } from '@llmcc/tools';

// Check if a tool is built-in
if (isBuiltinTool('read_file')) {
  // Execute the tool
  const result = await executeBuiltinTool('read_file', { path: './src/index.ts' });
  console.log(result.output);
}

// Get documentation for all tools
const docs = getBuiltinToolDocs();
```
