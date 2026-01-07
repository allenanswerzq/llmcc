/**
 * Browser Controller
 *
 * Wraps Puppeteer to provide a clean interface for browser automation.
 * Manages browser lifecycle, pages, and provides common operations.
 */
import { Page } from 'puppeteer';
export interface TabInfo {
    tabId: string;
    title: string;
    url: string;
}
export interface ConsoleMessage {
    type: string;
    text: string;
    timestamp: number;
}
export interface NetworkRequest {
    url: string;
    method: string;
    status?: number;
    timestamp: number;
}
export interface ScreenshotResult {
    data: string;
    width: number;
    height: number;
}
export declare class BrowserController {
    private browser;
    private pages;
    private consoleMessages;
    private networkRequests;
    private tabCounter;
    private headless;
    constructor(options?: {
        headless?: boolean;
    });
    /**
     * Initialize the browser
     */
    init(): Promise<void>;
    /**
     * Create a new tab
     */
    createTab(): Promise<string>;
    /**
     * Get all tabs info
     */
    getTabs(): Promise<TabInfo[]>;
    /**
     * Get page by tab ID
     */
    getPage(tabId: string): Page | undefined;
    /**
     * Navigate to URL
     */
    navigate(tabId: string, url: string): Promise<string>;
    /**
     * Get page text content
     */
    getPageText(tabId: string): Promise<string>;
    /**
     * Get accessibility tree (simplified)
     */
    getAccessibilityTree(tabId: string, depth?: number): Promise<string>;
    /**
     * Find elements by text query
     */
    findElements(tabId: string, query: string): Promise<Array<{
        role: string;
        text: string;
        x: number;
        y: number;
    }>>;
    /**
     * Take screenshot
     */
    screenshot(tabId: string): Promise<ScreenshotResult>;
    /**
     * Click at coordinates
     */
    click(tabId: string, x: number, y: number, button?: 'left' | 'right', clickCount?: number): Promise<string>;
    /**
     * Type text
     */
    type(tabId: string, text: string): Promise<string>;
    /**
     * Press key
     */
    pressKey(tabId: string, key: string, modifiers?: string[]): Promise<string>;
    /**
     * Scroll
     */
    scroll(tabId: string, x: number, y: number, direction: 'up' | 'down' | 'left' | 'right', amount?: number): Promise<string>;
    /**
     * Execute JavaScript
     */
    executeScript(tabId: string, script: string): Promise<unknown>;
    /**
     * Set viewport size
     */
    setViewport(tabId: string, width: number, height: number): Promise<string>;
    /**
     * Get console messages
     */
    getConsoleMessages(tabId: string, limit?: number, onlyErrors?: boolean): ConsoleMessage[];
    /**
     * Get network requests
     */
    getNetworkRequests(tabId: string, limit?: number, urlPattern?: string): NetworkRequest[];
    /**
     * Fill form input
     */
    fillInput(tabId: string, selector: string, value: string): Promise<string>;
    /**
     * Get page URL and title
     */
    getPageInfo(tabId: string): Promise<{
        url: string;
        title: string;
    }>;
    /**
     * Close a tab
     */
    closeTab(tabId: string): Promise<void>;
    /**
     * Close the browser
     */
    close(): Promise<void>;
}
export default BrowserController;
//# sourceMappingURL=browser-controller.d.ts.map