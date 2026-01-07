/**
 * Native Messaging Protocol Handler
 *
 * Chrome Native Messaging uses a simple protocol:
 * - 4-byte little-endian length prefix
 * - UTF-8 JSON payload
 *
 * This module handles reading/writing messages from stdin/stdout.
 */

import { EventEmitter } from 'events';

export interface NativeMessage {
    method?: string;
    params?: Record<string, unknown>;
    id?: string | number | null;
    result?: unknown;
    error?: { message: string; code?: number };
}

export class NativeMessagingHost extends EventEmitter {
    private buffer: Buffer = Buffer.alloc(0);
    private isRunning = false;

    constructor() {
        super();
    }

    /**
     * Start listening for messages on stdin
     */
    start(): void {
        if (this.isRunning) return;
        this.isRunning = true;

        // Set stdin to binary mode
        if (process.stdin.setRawMode) {
            process.stdin.setRawMode(true);
        }
        process.stdin.resume();

        process.stdin.on('data', (chunk: Buffer) => {
            this.handleData(chunk);
        });

        process.stdin.on('end', () => {
            this.emit('disconnect');
            this.isRunning = false;
        });

        process.stdin.on('error', (err) => {
            this.emit('error', err);
        });

        this.emit('ready');
    }

    /**
     * Handle incoming data chunks
     */
    private handleData(chunk: Buffer): void {
        this.buffer = Buffer.concat([this.buffer, chunk]);

        // Process complete messages
        while (this.buffer.length >= 4) {
            // Read 4-byte little-endian length
            const length = this.buffer.readUInt32LE(0);

            // Check if we have the complete message
            if (this.buffer.length < 4 + length) {
                break; // Wait for more data
            }

            // Extract the JSON payload
            const payload = this.buffer.subarray(4, 4 + length);
            this.buffer = this.buffer.subarray(4 + length);

            try {
                const message = JSON.parse(payload.toString('utf-8')) as NativeMessage;
                this.emit('message', message);
            } catch (err) {
                this.emit('error', new Error(`Failed to parse message: ${err}`));
            }
        }
    }

    /**
     * Send a message to stdout
     */
    send(message: NativeMessage): void {
        const payload = Buffer.from(JSON.stringify(message), 'utf-8');
        const length = Buffer.alloc(4);
        length.writeUInt32LE(payload.length, 0);

        process.stdout.write(Buffer.concat([length, payload]));
    }

    /**
     * Send a successful result
     */
    sendResult(id: string | number | undefined, result: unknown): void {
        this.send({ id, result });
    }

    /**
     * Send an error response
     */
    sendError(id: string | number | undefined, message: string, code?: number): void {
        this.send({ id, error: { message, code } });
    }

    /**
     * Stop the host
     */
    stop(): void {
        this.isRunning = false;
        process.stdin.pause();
    }
}

export default NativeMessagingHost;
