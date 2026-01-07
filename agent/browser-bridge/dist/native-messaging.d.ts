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
    error?: {
        message: string;
        code?: number;
    };
}
export declare class NativeMessagingHost extends EventEmitter {
    private buffer;
    private isRunning;
    constructor();
    /**
     * Start listening for messages on stdin
     */
    start(): void;
    /**
     * Handle incoming data chunks
     */
    private handleData;
    /**
     * Send a message to stdout
     */
    send(message: NativeMessage): void;
    /**
     * Send a successful result
     */
    sendResult(id: string | number | undefined, result: unknown): void;
    /**
     * Send an error response
     */
    sendError(id: string | number | undefined, message: string, code?: number): void;
    /**
     * Stop the host
     */
    stop(): void;
}
export default NativeMessagingHost;
//# sourceMappingURL=native-messaging.d.ts.map