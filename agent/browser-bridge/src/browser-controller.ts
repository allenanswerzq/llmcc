/**
 * Browser Controller
 *
 * Wraps Puppeteer to provide a clean interface for browser automation.
 * Manages browser lifecycle, pages, and provides common operations.
 */

import puppeteer, { Browser, Page, ElementHandle } from 'puppeteer';

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
    data: string;  // base64
    width: number;
    height: number;
}

export class BrowserController {
    private browser: Browser | null = null;
    private pages: Map<string, Page> = new Map();
    private consoleMessages: Map<string, ConsoleMessage[]> = new Map();
    private networkRequests: Map<string, NetworkRequest[]> = new Map();
    private tabCounter = 0;
    private headless: boolean;

    constructor(options: { headless?: boolean } = {}) {
        this.headless = options.headless ?? true;
    }

    /**
     * Initialize the browser
     */
    async init(): Promise<void> {
        if (this.browser) return;

        this.browser = await puppeteer.launch({
            headless: this.headless,
            args: [
                '--no-sandbox',
                '--disable-setuid-sandbox',
                '--disable-dev-shm-usage',
                '--disable-gpu',
            ],
        });

        // Create initial tab
        await this.createTab();
    }

    /**
     * Create a new tab
     */
    async createTab(): Promise<string> {
        if (!this.browser) await this.init();

        const page = await this.browser!.newPage();
        const tabId = `tab_${++this.tabCounter}`;

        // Set up console message capture
        this.consoleMessages.set(tabId, []);
        page.on('console', (msg) => {
            const messages = this.consoleMessages.get(tabId) || [];
            messages.push({
                type: msg.type(),
                text: msg.text(),
                timestamp: Date.now(),
            });
            // Keep last 1000 messages
            if (messages.length > 1000) messages.shift();
        });

        // Set up network request capture
        this.networkRequests.set(tabId, []);
        page.on('response', (response) => {
            const requests = this.networkRequests.get(tabId) || [];
            requests.push({
                url: response.url(),
                method: response.request().method(),
                status: response.status(),
                timestamp: Date.now(),
            });
            // Keep last 1000 requests
            if (requests.length > 1000) requests.shift();
        });

        // Set reasonable viewport
        await page.setViewport({ width: 1280, height: 800 });

        this.pages.set(tabId, page);
        return tabId;
    }

    /**
     * Get all tabs info
     */
    async getTabs(): Promise<TabInfo[]> {
        const tabs: TabInfo[] = [];
        for (const [tabId, page] of this.pages) {
            tabs.push({
                tabId,
                title: await page.title(),
                url: page.url(),
            });
        }
        return tabs;
    }

    /**
     * Get page by tab ID
     */
    getPage(tabId: string): Page | undefined {
        return this.pages.get(tabId);
    }

    /**
     * Navigate to URL
     */
    async navigate(tabId: string, url: string): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        await page.goto(url, { waitUntil: 'networkidle2', timeout: 30000 });
        return `Navigated to ${url}`;
    }

    /**
     * Get page text content
     */
    async getPageText(tabId: string): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        return await page.evaluate(() => document.body.innerText);
    }

    /**
     * Get accessibility tree (simplified)
     */
    async getAccessibilityTree(tabId: string, depth: number = 15): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        // Get accessibility snapshot
        const snapshot = await page.accessibility.snapshot({ root: undefined });

        if (!snapshot) {
            return 'No accessibility tree available';
        }

        // Format tree
        const formatNode = (node: any, indent: number = 0, refCounter = { count: 0 }): string => {
            if (indent > depth) return '';

            const ref = `ref_${++refCounter.count}`;
            const prefix = '  '.repeat(indent);
            let result = `${prefix}${ref} ${node.role || 'unknown'}`;

            if (node.name) result += `: ${node.name}`;
            if (node.value) result += ` [value: ${node.value}]`;
            result += '\n';

            if (node.children) {
                for (const child of node.children) {
                    result += formatNode(child, indent + 1, refCounter);
                }
            }
            return result;
        };

        return `Accessibility tree:\n${formatNode(snapshot)}`;
    }

    /**
     * Find elements by text query
     */
    async findElements(tabId: string, query: string): Promise<Array<{
        role: string;
        text: string;
        x: number;
        y: number;
    }>> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        const results = await page.evaluate((q) => {
            const matches: Array<{ role: string; text: string; x: number; y: number }> = [];
            const queryLower = q.toLowerCase();

            const elements = Array.from(document.querySelectorAll(
                'a, button, input, select, textarea, [role="button"], [role="link"], [onclick], [tabindex]'
            ));

            for (const el of elements) {
                const text = (el.textContent || '').trim();
                const ariaLabel = el.getAttribute('aria-label') || '';
                const placeholder = (el as HTMLInputElement).placeholder || '';

                if (
                    text.toLowerCase().includes(queryLower) ||
                    ariaLabel.toLowerCase().includes(queryLower) ||
                    placeholder.toLowerCase().includes(queryLower)
                ) {
                    const rect = el.getBoundingClientRect();
                    if (rect.width > 0 && rect.height > 0) {
                        matches.push({
                            role: el.getAttribute('role') || el.tagName.toLowerCase(),
                            text: text.substring(0, 50) || ariaLabel || placeholder,
                            x: Math.round(rect.x + rect.width / 2),
                            y: Math.round(rect.y + rect.height / 2),
                        });
                    }
                }
                if (matches.length >= 20) break;
            }
            return matches;
        }, query);

        return results;
    }

    /**
     * Take screenshot
     */
    async screenshot(tabId: string): Promise<ScreenshotResult> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        const viewport = page.viewport();
        const buffer = await page.screenshot({ type: 'jpeg', quality: 80 }) as Buffer;

        return {
            data: Buffer.from(buffer).toString('base64'),
            width: viewport?.width || 1280,
            height: viewport?.height || 800,
        };
    }

    /**
     * Click at coordinates
     */
    async click(tabId: string, x: number, y: number, button: 'left' | 'right' = 'left', clickCount: number = 1): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        await page.mouse.click(x, y, { button, clickCount });
        return `Clicked at (${x}, ${y})`;
    }

    /**
     * Type text
     */
    async type(tabId: string, text: string): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        await page.keyboard.type(text);
        return `Typed: ${text}`;
    }

    /**
     * Press key
     */
    async pressKey(tabId: string, key: string, modifiers: string[] = []): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        // Build key combo
        let combo = key;
        if (modifiers.length > 0) {
            combo = [...modifiers, key].join('+');
        }

        await page.keyboard.press(combo as any);
        return `Pressed: ${combo}`;
    }

    /**
     * Scroll
     */
    async scroll(tabId: string, x: number, y: number, direction: 'up' | 'down' | 'left' | 'right', amount: number = 3): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        const deltaX = direction === 'left' ? -100 * amount : direction === 'right' ? 100 * amount : 0;
        const deltaY = direction === 'up' ? -100 * amount : direction === 'down' ? 100 * amount : 0;

        await page.mouse.move(x, y);
        await page.mouse.wheel({ deltaX, deltaY });
        return `Scrolled ${direction} by ${amount}`;
    }

    /**
     * Execute JavaScript
     */
    async executeScript(tabId: string, script: string): Promise<unknown> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        return await page.evaluate(script);
    }

    /**
     * Set viewport size
     */
    async setViewport(tabId: string, width: number, height: number): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        await page.setViewport({ width, height });
        return `Viewport set to ${width}x${height}`;
    }

    /**
     * Get console messages
     */
    getConsoleMessages(tabId: string, limit: number = 100, onlyErrors: boolean = false): ConsoleMessage[] {
        let messages = this.consoleMessages.get(tabId) || [];
        if (onlyErrors) {
            messages = messages.filter(m => m.type === 'error');
        }
        return messages.slice(-limit);
    }

    /**
     * Get network requests
     */
    getNetworkRequests(tabId: string, limit: number = 100, urlPattern?: string): NetworkRequest[] {
        let requests = this.networkRequests.get(tabId) || [];
        if (urlPattern) {
            requests = requests.filter(r => r.url.includes(urlPattern));
        }
        return requests.slice(-limit);
    }

    /**
     * Fill form input
     */
    async fillInput(tabId: string, selector: string, value: string): Promise<string> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        await page.type(selector, value);
        return `Filled input with: ${value}`;
    }

    /**
     * Get page URL and title
     */
    async getPageInfo(tabId: string): Promise<{ url: string; title: string }> {
        const page = this.pages.get(tabId);
        if (!page) throw new Error(`Tab not found: ${tabId}`);

        return {
            url: page.url(),
            title: await page.title(),
        };
    }

    /**
     * Close a tab
     */
    async closeTab(tabId: string): Promise<void> {
        const page = this.pages.get(tabId);
        if (page) {
            await page.close();
            this.pages.delete(tabId);
            this.consoleMessages.delete(tabId);
            this.networkRequests.delete(tabId);
        }
    }

    /**
     * Close the browser
     */
    async close(): Promise<void> {
        if (this.browser) {
            await this.browser.close();
            this.browser = null;
            this.pages.clear();
            this.consoleMessages.clear();
            this.networkRequests.clear();
        }
    }
}

export default BrowserController;
