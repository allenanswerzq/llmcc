import * as vscode from 'vscode';

export class StatusBarManager implements vscode.Disposable {
    private statusBarItem: vscode.StatusBarItem;

    constructor() {
        this.statusBarItem = vscode.window.createStatusBarItem(
            vscode.StatusBarAlignment.Right,
            100
        );
        this.statusBarItem.command = 'copilot-api-bridge.status';
        this.setStopped();
        this.statusBarItem.show();
    }

    setRunning(port: number): void {
        this.statusBarItem.text = `$(radio-tower) API Bridge :${port}`;
        this.statusBarItem.tooltip = `Copilot API Bridge running on port ${port}\nClick for options`;
        this.statusBarItem.backgroundColor = undefined;
    }

    setStopped(): void {
        this.statusBarItem.text = '$(circle-slash) API Bridge';
        this.statusBarItem.tooltip = 'Copilot API Bridge is stopped\nClick to start';
        this.statusBarItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
    }

    dispose(): void {
        this.statusBarItem.dispose();
    }
}
