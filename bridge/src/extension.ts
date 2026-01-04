import * as vscode from 'vscode';
import { ApiServer } from './server';
import { StatusBarManager } from './statusBar';

let server: ApiServer | undefined;
let statusBar: StatusBarManager | undefined;

export async function activate(context: vscode.ExtensionContext) {
    console.log('Copilot API Bridge is activating...');

    statusBar = new StatusBarManager();
    context.subscriptions.push(statusBar);

    // Register commands
    context.subscriptions.push(
        vscode.commands.registerCommand('copilot-api-bridge.start', async () => {
            await startServer();
        }),
        vscode.commands.registerCommand('copilot-api-bridge.stop', async () => {
            await stopServer();
        }),
        vscode.commands.registerCommand('copilot-api-bridge.status', () => {
            showStatus();
        })
    );

    // Auto-start if configured
    const config = vscode.workspace.getConfiguration('copilot-api-bridge');
    if (config.get<boolean>('autoStart', true)) {
        await startServer();
    }
}

async function startServer(): Promise<void> {
    if (server?.isRunning()) {
        vscode.window.showInformationMessage('API Bridge server is already running');
        return;
    }

    const config = vscode.workspace.getConfiguration('copilot-api-bridge');
    const port = config.get<number>('port', 5168);
    const bindAddress = config.get<string>('bindAddress', '0.0.0.0');

    try {
        server = new ApiServer(port, bindAddress);
        await server.start();
        statusBar?.setRunning(port);
        const address = bindAddress === '0.0.0.0' ? 'all interfaces' : bindAddress;
        vscode.window.showInformationMessage(`Copilot API Bridge started on ${address}:${port}`);
    } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        vscode.window.showErrorMessage(`Failed to start API Bridge: ${message}`);
    }
}

async function stopServer(): Promise<void> {
    if (!server?.isRunning()) {
        vscode.window.showInformationMessage('API Bridge server is not running');
        return;
    }

    try {
        await server.stop();
        statusBar?.setStopped();
        vscode.window.showInformationMessage('Copilot API Bridge stopped');
    } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        vscode.window.showErrorMessage(`Failed to stop API Bridge: ${message}`);
    }
}

function showStatus(): void {
    if (server?.isRunning()) {
        const port = vscode.workspace.getConfiguration('copilot-api-bridge').get<number>('port', 5168);
        vscode.window.showInformationMessage(
            `Copilot API Bridge is running on http://localhost:${port}`,
            'Open in Browser'
        ).then(selection => {
            if (selection === 'Open in Browser') {
                vscode.env.openExternal(vscode.Uri.parse(`http://localhost:${port}`));
            }
        });
    } else {
        vscode.window.showInformationMessage('Copilot API Bridge is stopped', 'Start Server')
            .then(selection => {
                if (selection === 'Start Server') {
                    vscode.commands.executeCommand('copilot-api-bridge.start');
                }
            });
    }
}

export function deactivate() {
    if (server?.isRunning()) {
        server.stop();
    }
}
