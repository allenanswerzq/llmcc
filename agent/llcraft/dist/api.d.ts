/**
 * API client for llcraft
 * Supports both Anthropic API and OpenAI-compatible APIs (like Copilot bridge)
 * With tool calling support including browser tools
 */
import { Message, Config } from './types.js';
/**
 * Call API - auto-detects Anthropic vs OpenAI format based on URL
 * Handles tool calling with automatic execution loop
 */
export declare function callAPI(messages: Message[], config: Config, onToolCall?: (name: string, args: string) => void): Promise<string>;
