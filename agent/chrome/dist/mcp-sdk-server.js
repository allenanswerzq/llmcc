#!/usr/bin/env node
/**
 * MCP Browser Bridge Server
 *
 * Uses the official MCP SDK for proper protocol compatibility.
 * Provides browser automation tools via Puppeteer.
 */
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema, } from '@modelcontextprotocol/sdk/types.js';
import { BrowserController } from './browser-controller.js';
async function main() {
    const args = process.argv.slice(2);
    const headless = !args.includes('--no-headless');
    const debug = args.includes('--debug');
    const log = (message) => {
        if (debug) {
            process.stderr.write(`[browser-bridge] ${message}\n`);
        }
    };
    log('Starting browser bridge MCP server...');
    // Initialize browser controller
    const browser = new BrowserController({ headless });
    let activeTabId = '';
    // Create MCP server
    const server = new Server({
        name: 'browser-bridge',
        version: '1.0.0',
    }, {
        capabilities: {
            tools: {},
        },
    });
    // Define available tools
    const tools = [
        {
            name: 'navigate',
            description: 'Navigate to a URL in the browser',
            inputSchema: {
                type: 'object',
                properties: {
                    url: { type: 'string', description: 'URL to navigate to' },
                },
                required: ['url'],
            },
        },
        {
            name: 'get_page_text',
            description: 'Get the text content of the current page',
            inputSchema: {
                type: 'object',
                properties: {},
            },
        },
        {
            name: 'screenshot',
            description: 'Take a screenshot of the current page',
            inputSchema: {
                type: 'object',
                properties: {},
            },
        },
        {
            name: 'click',
            description: 'Click at coordinates on the page',
            inputSchema: {
                type: 'object',
                properties: {
                    x: { type: 'number', description: 'X coordinate' },
                    y: { type: 'number', description: 'Y coordinate' },
                },
                required: ['x', 'y'],
            },
        },
        {
            name: 'type_text',
            description: 'Type text at the current cursor position',
            inputSchema: {
                type: 'object',
                properties: {
                    text: { type: 'string', description: 'Text to type' },
                },
                required: ['text'],
            },
        },
        {
            name: 'scroll',
            description: 'Scroll the page',
            inputSchema: {
                type: 'object',
                properties: {
                    direction: { type: 'string', enum: ['up', 'down', 'left', 'right'], description: 'Scroll direction' },
                    amount: { type: 'number', description: 'Scroll amount in pixels', default: 300 },
                },
                required: ['direction'],
            },
        },
        {
            name: 'read_page',
            description: 'Read the page accessibility tree',
            inputSchema: {
                type: 'object',
                properties: {
                    depth: { type: 'number', description: 'Max depth of tree', default: 15 },
                },
            },
        },
        {
            name: 'find_element',
            description: 'Find an interactive element by text',
            inputSchema: {
                type: 'object',
                properties: {
                    query: { type: 'string', description: 'Text to search for' },
                },
                required: ['query'],
            },
        },
        {
            name: 'execute_javascript',
            description: 'Execute JavaScript in the page context',
            inputSchema: {
                type: 'object',
                properties: {
                    script: { type: 'string', description: 'JavaScript code to execute' },
                },
                required: ['script'],
            },
        },
    ];
    // Handle tools/list request
    server.setRequestHandler(ListToolsRequestSchema, async () => {
        log('Received tools/list request');
        return { tools };
    });
    // Handle tools/call request
    server.setRequestHandler(CallToolRequestSchema, async (request) => {
        const { name, arguments: args } = request.params;
        log(`Received tools/call for ${name}: ${JSON.stringify(args)}`);
        try {
            // Initialize browser if needed
            if (!activeTabId) {
                await browser.init();
                const tabs = await browser.getTabs();
                if (tabs.length > 0) {
                    activeTabId = tabs[0].tabId;
                }
            }
            let result;
            switch (name) {
                case 'navigate': {
                    result = await browser.navigate(activeTabId, args?.url);
                    break;
                }
                case 'get_page_text': {
                    const text = await browser.getPageText(activeTabId);
                    result = text;
                    break;
                }
                case 'screenshot': {
                    const screenshotResult = await browser.screenshot(activeTabId);
                    return {
                        content: [
                            {
                                type: 'image',
                                data: screenshotResult.data,
                                mimeType: 'image/jpeg',
                            },
                        ],
                    };
                }
                case 'click': {
                    result = await browser.click(activeTabId, args?.x, args?.y);
                    break;
                }
                case 'type_text': {
                    result = await browser.type(activeTabId, args?.text);
                    break;
                }
                case 'scroll': {
                    const direction = args?.direction;
                    const amount = args?.amount || 300;
                    // Get viewport center for scroll position
                    result = await browser.scroll(activeTabId, 500, 400, direction, amount);
                    break;
                }
                case 'read_page': {
                    const tree = await browser.getAccessibilityTree(activeTabId, args?.depth);
                    result = tree;
                    break;
                }
                case 'find_element': {
                    const elements = await browser.findElements(activeTabId, args?.query);
                    result = JSON.stringify(elements, null, 2);
                    break;
                }
                case 'execute_javascript': {
                    const jsResult = await browser.executeScript(activeTabId, args?.script);
                    result = JSON.stringify(jsResult);
                    break;
                }
                default:
                    throw new Error(`Unknown tool: ${name}`);
            }
            return {
                content: [
                    {
                        type: 'text',
                        text: result,
                    },
                ],
            };
        }
        catch (error) {
            log(`Error executing ${name}: ${error}`);
            return {
                content: [
                    {
                        type: 'text',
                        text: `Error: ${error instanceof Error ? error.message : String(error)}`,
                    },
                ],
                isError: true,
            };
        }
    });
    // Handle shutdown
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
    // Start the server with stdio transport
    const transport = new StdioServerTransport();
    await server.connect(transport);
    log('Browser bridge MCP server running');
}
main().catch((error) => {
    process.stderr.write(`Fatal error: ${error.message}\n`);
    process.exit(1);
});
//# sourceMappingURL=mcp-sdk-server.js.map