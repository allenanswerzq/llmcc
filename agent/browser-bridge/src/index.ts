#!/usr/bin/env node
/**
 * Browser Bridge Entry Point
 *
 * This is a Native Messaging host that provides browser automation tools
 * to Claude Code. It implements the MCP protocol over Native Messaging.
 */

import { NativeMessagingHost } from './native-messaging.js';
import { BrowserController } from './browser-controller.js';
import { MCPServer, MCPRequest } from './mcp-server.js';

async function main() {
    // Parse arguments
    const args = process.argv.slice(2);
    const headless = !args.includes('--no-headless');
    const debug = args.includes('--debug');

    // Set up logging
    const log = (message: string) => {
        if (debug) {
            process.stderr.write(`[browser-bridge] ${message}\n`);
        }
    };

    log('Starting browser bridge...');

    // Initialize components
    const browser = new BrowserController({ headless });
    const mcp = new MCPServer(browser);
    const messaging = new NativeMessagingHost();

    // Handle incoming messages
    messaging.on('message', async (msg: unknown) => {
        log(`Received: ${JSON.stringify(msg)}`);

        const request = msg as MCPRequest;
        const response = await mcp.handleRequest(request);

        log(`Responding: ${JSON.stringify(response)}`);
        messaging.send(response);
    });

    // Handle disconnect
    messaging.on('disconnect', async () => {
        log('Client disconnected, shutting down...');
        await browser.close();
        process.exit(0);
    });

    // Handle errors
    messaging.on('error', (error: Error) => {
        log(`Error: ${error.message}`);
    });

    // Clean shutdown
    process.on('SIGINT', async () => {
        log('SIGINT received, shutting down...');
        await browser.close();
        process.exit(0);
    });

    process.on('SIGTERM', async () => {
        log('SIGTERM received, shutting down...');
        await browser.close();
        process.exit(0);
    });

    // Start listening
    messaging.start();
    log('Browser bridge ready');
}

main().catch((error) => {
    process.stderr.write(`Fatal error: ${error.message}\n`);
    process.exit(1);
});
