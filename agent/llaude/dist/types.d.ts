/**
 * Type definitions for llaude
 */
export interface Message {
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: string;
}
export interface Session {
    id: string;
    created: string;
    messages: Message[];
    metadata?: Record<string, unknown>;
}
export interface Config {
    apiKey: string;
    baseUrl: string;
    model: string;
    maxTokens: number;
    systemPrompt: string;
}
export interface APIMessage {
    role: 'user' | 'assistant';
    content: string;
}
export interface APIRequest {
    model: string;
    max_tokens: number;
    system?: string;
    messages: APIMessage[];
    stream?: boolean;
}
export interface ContentBlock {
    type: 'text';
    text: string;
}
export interface APIResponse {
    id: string;
    type: 'message';
    role: 'assistant';
    content: ContentBlock[];
    model: string;
    stop_reason: string;
    usage: {
        input_tokens: number;
        output_tokens: number;
    };
}
export interface CommandResult {
    session: Session;
    config: Config;
    output?: string;
    shouldExit: boolean;
}
