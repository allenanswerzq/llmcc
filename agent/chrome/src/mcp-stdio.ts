#!/usr/bin/env node
/**
 * MCP Stdio Transport Entry Point
 *
 * This is an MCP server that uses stdio transport (newline-delimited JSON-RPC).
 * Claude Code uses this transport when running MCP servers.
 */

import * as readline from 'readline';
import { BrowserController } from './browser-controller.js';
import { MCPServer, MCPRequest, MCPResponse } from './mcp-server.js';

async function main() {
    // Parse arguments
    const args = process.argv.slice(2);
    const headless = !args.includes('--no-headless');
    const debug = args.includes('--debug');

    // Set up logging (to stderr, since stdout is for MCP protocol)
    const log = (message: string) => {
        if (debug) {
            process.stderr.write(`[browser-bridge] ${message}\n`);
        }
    };

    log('Starting MCP browser bridge (stdio transport)...');

    // Initialize components
    const browser = new BrowserController({ headless });
    const mcp = new MCPServer(browser);

    // Send a response as a single line of JSON to stdout
    const sendResponse = (response: MCPResponse) => {
        const json = JSON.stringify(response);
        log(`Sending: ${json}`);
        process.stdout.write(json + '\n');
    };

    // Create readline interface for newline-delimited JSON
    // Don't pass output to avoid interfering with stdout
    const rl = readline.createInterface({
        input: process.stdin,
        terminal: false
    });

    // Track pending requests to ensure they complete before shutdown
    let pendingRequests = 0;
    let shuttingDown = false;

    const tryShutdown = async () => {
        if (shuttingDown && pendingRequests === 0) {
            log('All requests complete, shutting down...');
            await browser.close();
            process.exit(0);
        }
    };

    // Handle each line of input
    rl.on('line', (line: string) => {
        if (!line.trim()) return;

        log(`Received: ${line}`);
        pendingRequests++;

        (async () => {
            try {
                const request = JSON.parse(line) as MCPRequest;
                const response = await mcp.handleRequest(request);
                sendResponse(response);
            } catch (error) {
                log(`Error parsing request: ${error}`);
                sendResponse({
                    jsonrpc: '2.0',
                    id: null,
                    error: {
                        code: -32700,
                        message: `Parse error: ${error instanceof Error ? error.message : String(error)}`
                    }
                });
            } finally {
                pendingRequests--;
                tryShutdown();
            }
        })();
    });

    // Handle disconnect
    rl.on('close', () => {
        log('Client disconnected...');
        shuttingDown = true;
        tryShutdown();
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

    log('MCP browser bridge ready (waiting for stdin)');
}

main().catch((error) => {
    process.stderr.write(`Fatal error: ${error.message}\n`);
    process.exit(1);
});
