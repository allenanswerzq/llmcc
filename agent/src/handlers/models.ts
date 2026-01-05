import * as vscode from 'vscode';
import type { ModelsResponse, ModelInfo } from '../types';

export async function handleModels(): Promise<ModelsResponse> {
    // Get available models from VS Code Language Model API
    const availableModels = await vscode.lm.selectChatModels({});

    const models: ModelInfo[] = availableModels.map(model => ({
        id: model.id,
        object: 'model' as const,
        created: Math.floor(Date.now() / 1000),
        owned_by: model.vendor,
    }));

    // Also add common aliases that map to available models
    const aliases: ModelInfo[] = [
        { id: 'gpt-4o', object: 'model', created: Math.floor(Date.now() / 1000), owned_by: 'copilot' },
        { id: 'gpt-4', object: 'model', created: Math.floor(Date.now() / 1000), owned_by: 'copilot' },
        { id: 'claude-3.5-sonnet', object: 'model', created: Math.floor(Date.now() / 1000), owned_by: 'copilot' },
        { id: 'claude-opus-4', object: 'model', created: Math.floor(Date.now() / 1000), owned_by: 'copilot' },
        { id: 'o1', object: 'model', created: Math.floor(Date.now() / 1000), owned_by: 'copilot' },
        { id: 'o1-mini', object: 'model', created: Math.floor(Date.now() / 1000), owned_by: 'copilot' },
    ];

    // Combine and deduplicate
    const allModels = [...models];
    for (const alias of aliases) {
        if (!allModels.find(m => m.id === alias.id)) {
            allModels.push(alias);
        }
    }

    return {
        object: 'list',
        data: allModels,
    };
}
