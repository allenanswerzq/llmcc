// Agent loop for tool execution
// This handles the back-and-forth of tool calling with the LLM

import * as vscode from 'vscode';
import {
    executeTool,
    parseToolCalls,
    generateToolSystemPrompt,
    ToolDefinition,
    ToolResult
} from './index';

export interface AgentMessage {
    role: 'user' | 'assistant' | 'system' | 'tool';
    content: string;
    toolCallId?: string;
    toolName?: string;
}

export interface AgentLoopResult {
    messages: AgentMessage[];
    finalResponse: string;
    toolCalls: Array<{
        name: string;
        arguments: Record<string, unknown>;
        result: ToolResult;
    }>;
}

const MAX_TOOL_ITERATIONS = 10;

export async function runAgentLoop(
    model: vscode.LanguageModelChat,
    messages: vscode.LanguageModelChatMessage[],
    tools: ToolDefinition[],
    onChunk?: (text: string) => void
): Promise<AgentLoopResult> {
    const result: AgentLoopResult = {
        messages: [],
        finalResponse: '',
        toolCalls: []
    };

    // Add tool system prompt
    const toolSystemPrompt = generateToolSystemPrompt(tools);
    const systemMessage = vscode.LanguageModelChatMessage.User(`[System]: ${toolSystemPrompt}`);
    const augmentedMessages = [systemMessage, ...messages];

    let currentMessages = [...augmentedMessages];
    let iterations = 0;

    while (iterations < MAX_TOOL_ITERATIONS) {
        iterations++;
        console.log(`[Agent] Iteration ${iterations}`);

        // Send request to model
        const response = await model.sendRequest(currentMessages, {});

        // Collect response
        let responseText = '';
        for await (const chunk of response.text) {
            responseText += chunk;
            onChunk?.(chunk);
        }

        console.log(`[Agent] Model response: ${responseText.substring(0, 200)}...`);

        // Check for tool calls in response
        const toolCalls = parseToolCalls(responseText);

        if (toolCalls.length === 0) {
            // No tool calls, we're done
            result.finalResponse = responseText;
            result.messages.push({ role: 'assistant', content: responseText });
            break;
        }

        // Execute tool calls
        for (const call of toolCalls) {
            console.log(`[Agent] Executing tool: ${call.name}`);
            const toolResult = await executeTool(call.name, call.arguments);

            result.toolCalls.push({
                name: call.name,
                arguments: call.arguments,
                result: toolResult
            });

            // Add assistant message with tool call
            result.messages.push({
                role: 'assistant',
                content: responseText
            });

            // Add tool result as user message (since VS Code API doesn't have tool role)
            const toolResultText = toolResult.success
                ? `[Tool Result for ${call.name}]: ${toolResult.output}`
                : `[Tool Error for ${call.name}]: ${toolResult.error}`;

            result.messages.push({
                role: 'tool',
                content: toolResultText,
                toolCallId: `call_${Date.now()}`,
                toolName: call.name
            });

            // Add to current messages for next iteration
            currentMessages.push(
                vscode.LanguageModelChatMessage.Assistant(responseText)
            );
            currentMessages.push(
                vscode.LanguageModelChatMessage.User(toolResultText)
            );

            // Stream tool result to client if callback provided
            onChunk?.(`\n${toolResultText}\n`);
        }
    }

    if (iterations >= MAX_TOOL_ITERATIONS) {
        console.log(`[Agent] Max iterations reached`);
        result.finalResponse += '\n[Max tool iterations reached]';
    }

    return result;
}

// Simpler version that just executes one round of tools
export async function executeToolsFromResponse(
    responseText: string
): Promise<{
    hasToolCalls: boolean;
    toolResults: Array<{ name: string; result: ToolResult }>;
    combinedOutput: string;
}> {
    const toolCalls = parseToolCalls(responseText);

    if (toolCalls.length === 0) {
        return {
            hasToolCalls: false,
            toolResults: [],
            combinedOutput: responseText
        };
    }

    const toolResults: Array<{ name: string; result: ToolResult }> = [];
    const outputs: string[] = [responseText];

    for (const call of toolCalls) {
        const result = await executeTool(call.name, call.arguments);
        toolResults.push({ name: call.name, result });

        if (result.success) {
            outputs.push(`\n[${call.name} output]:\n${result.output}`);
        } else {
            outputs.push(`\n[${call.name} error]:\n${result.error}`);
        }
    }

    return {
        hasToolCalls: true,
        toolResults,
        combinedOutput: outputs.join('\n')
    };
}
