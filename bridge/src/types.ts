import * as vscode from 'vscode';

// OpenAI-compatible types
export interface ChatCompletionRequest {
    model: string;
    messages: ChatMessage[];
    temperature?: number;
    max_tokens?: number;
    stream?: boolean;
    top_p?: number;
    stop?: string | string[];
}

export interface ChatMessage {
    role: 'system' | 'user' | 'assistant';
    content: string;
}

export interface ChatCompletionResponse {
    id: string;
    object: 'chat.completion';
    created: number;
    model: string;
    choices: ChatCompletionChoice[];
    usage: {
        prompt_tokens: number;
        completion_tokens: number;
        total_tokens: number;
    };
}

export interface ChatCompletionChoice {
    index: number;
    message: {
        role: 'assistant';
        content: string;
    };
    finish_reason: 'stop' | 'length' | 'content_filter' | null;
}

export interface ChatCompletionChunk {
    id: string;
    object: 'chat.completion.chunk';
    created: number;
    model: string;
    choices: {
        index: number;
        delta: {
            role?: 'assistant';
            content?: string;
        };
        finish_reason: 'stop' | 'length' | null;
    }[];
}

export interface ModelInfo {
    id: string;
    object: 'model';
    created: number;
    owned_by: string;
}

export interface ModelsResponse {
    object: 'list';
    data: ModelInfo[];
}

// Model mapping from OpenAI-style names to VS Code LM selectors
export const MODEL_MAPPING: Record<string, { vendor: string; family: string }> = {
    // Claude models
    'claude-opus-4.5': { vendor: 'copilot', family: 'claude-opus-4.5' },
    'claude-opus-4': { vendor: 'copilot', family: 'claude-opus-4' },
    'claude-4-opus': { vendor: 'copilot', family: 'claude-opus-4' },
    'claude-sonnet-4.5': { vendor: 'copilot', family: 'claude-sonnet-4.5' },
    'claude-sonnet-4': { vendor: 'copilot', family: 'claude-sonnet-4' },
    'claude-haiku-4.5': { vendor: 'copilot', family: 'claude-haiku-4.5' },
    'claude-3.5-sonnet': { vendor: 'copilot', family: 'claude-3.5-sonnet' },
    'claude-3-opus': { vendor: 'copilot', family: 'claude-opus-4' },

    // GPT-5.x models
    'gpt-5.2': { vendor: 'copilot', family: 'gpt-5.2' },
    'gpt-5.1-codex-max': { vendor: 'copilot', family: 'gpt-5.1-codex-max' },
    'gpt-5.1-codex-mini': { vendor: 'copilot', family: 'gpt-5.1-codex-mini' },
    'gpt-5.1-codex': { vendor: 'copilot', family: 'gpt-5.1-codex' },
    'gpt-5.1': { vendor: 'copilot', family: 'gpt-5.1' },
    'gpt-5-codex': { vendor: 'copilot', family: 'gpt-5-codex' },
    'gpt-5-mini': { vendor: 'copilot', family: 'gpt-5-mini' },
    'gpt-5': { vendor: 'copilot', family: 'gpt-5' },

    // GPT-4.x models
    'gpt-4.1': { vendor: 'copilot', family: 'gpt-4.1' },
    'gpt-4o-mini': { vendor: 'copilot', family: 'gpt-4o-mini' },
    'gpt-4o': { vendor: 'copilot', family: 'gpt-4o' },
    'gpt-4-turbo': { vendor: 'copilot', family: 'gpt-4o' },
    'gpt-4': { vendor: 'copilot', family: 'gpt-4' },
    'gpt-3.5-turbo': { vendor: 'copilot', family: 'gpt-4o' },

    // Gemini models
    'gemini-3-pro-preview': { vendor: 'copilot', family: 'gemini-3-pro-preview' },
    'gemini-3-flash-preview': { vendor: 'copilot', family: 'gemini-3-flash-preview' },
    'gemini-2.5-pro': { vendor: 'copilot', family: 'gemini-2.5-pro' },

    // O1 models
    'o1': { vendor: 'copilot', family: 'o1' },
    'o1-preview': { vendor: 'copilot', family: 'o1' },
    'o1-mini': { vendor: 'copilot', family: 'o1-mini' },

    // Special models
    'auto': { vendor: 'copilot', family: 'auto' },
    'copilot-fast': { vendor: 'copilot', family: 'copilot-fast' },

    // Default/catch-all
    'default': { vendor: 'copilot', family: 'claude-opus-4.5' },
};

export function getModelSelector(modelName: string): { vendor: string; family: string } {
    const normalized = modelName.toLowerCase().trim();

    // Direct match
    if (MODEL_MAPPING[normalized]) {
        return MODEL_MAPPING[normalized];
    }

    // Fuzzy match
    for (const [key, value] of Object.entries(MODEL_MAPPING)) {
        if (normalized.includes(key) || key.includes(normalized)) {
            return value;
        }
    }

    // Get default from config
    const config = vscode.workspace.getConfiguration('copilot-api-bridge');
    const defaultModel = config.get<string>('defaultModel', 'claude-sonnet-4');

    return MODEL_MAPPING[defaultModel] || MODEL_MAPPING['default'];
}

export function generateRequestId(): string {
    return 'chatcmpl-' + Math.random().toString(36).substring(2, 15);
}
