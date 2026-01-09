#!/usr/bin/env node
/**
 * Test script for Native Messaging protocol
 *
 * This simulates what Claude Code does when connecting to the browser bridge.
 * Run this to verify the bridge is working correctly.
 */

import { spawn } from 'child_process';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const bridgePath = join(__dirname, '..', 'dist', 'index.js');

// Send a message using Native Messaging protocol (4-byte length + JSON)
function sendMessage(proc, message) {
    const json = JSON.stringify(message);
    const buffer = Buffer.alloc(4 + json.length);
    buffer.writeUInt32LE(json.length, 0);
    buffer.write(json, 4);
    proc.stdin.write(buffer);
}

// Parse a Native Messaging response
function parseResponse(buffer) {
    if (buffer.length < 4) return null;
    const length = buffer.readUInt32LE(0);
    if (buffer.length < 4 + length) return null;
    const json = buffer.slice(4, 4 + length).toString('utf8');
    return JSON.parse(json);
}

async function main() {
    console.log('Starting browser bridge...');

    const proc = spawn('node', [bridgePath, '--debug'], {
        stdio: ['pipe', 'pipe', 'inherit'],
    });

    let buffer = Buffer.alloc(0);

    proc.stdout.on('data', (data) => {
        buffer = Buffer.concat([buffer, data]);

        while (true) {
            const response = parseResponse(buffer);
            if (!response) break;

            const length = buffer.readUInt32LE(0);
            buffer = buffer.slice(4 + length);

            console.log('Response:', JSON.stringify(response, null, 2));
        }
    });

    // Wait a bit for startup
    await new Promise(r => setTimeout(r, 1000));

    // Test 1: Initialize
    console.log('\n--- Test: Initialize ---');
    sendMessage(proc, {
        id: 1,
        jsonrpc: '2.0',
        method: 'initialize',
        params: {},
    });

    await new Promise(r => setTimeout(r, 2000));

    // Test 2: List tools
    console.log('\n--- Test: List Tools ---');
    sendMessage(proc, {
        id: 2,
        jsonrpc: '2.0',
        method: 'tools/list',
        params: {},
    });

    await new Promise(r => setTimeout(r, 1000));

    // Test 3: Navigate
    console.log('\n--- Test: Navigate ---');
    sendMessage(proc, {
        id: 3,
        jsonrpc: '2.0',
        method: 'tools/call',
        params: {
            name: 'navigate',
            arguments: { url: 'https://example.com' },
        },
    });

    await new Promise(r => setTimeout(r, 3000));

    // Test 4: Read page
    console.log('\n--- Test: Read Page ---');
    sendMessage(proc, {
        id: 4,
        jsonrpc: '2.0',
        method: 'tools/call',
        params: {
            name: 'get_page_text',
            arguments: {},
        },
    });

    await new Promise(r => setTimeout(r, 2000));

    // Test 5: Screenshot
    console.log('\n--- Test: Screenshot ---');
    sendMessage(proc, {
        id: 5,
        jsonrpc: '2.0',
        method: 'tools/call',
        params: {
            name: 'computer',
            arguments: { action: 'screenshot' },
        },
    });

    await new Promise(r => setTimeout(r, 2000));

    // Shutdown
    console.log('\n--- Test: Shutdown ---');
    sendMessage(proc, {
        id: 6,
        jsonrpc: '2.0',
        method: 'shutdown',
        params: {},
    });

    await new Promise(r => setTimeout(r, 1000));
    proc.kill();

    console.log('\nTests complete!');
}

main().catch(console.error);
