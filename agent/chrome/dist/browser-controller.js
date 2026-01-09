/**
 * Browser Controller
 *
 * Wraps Puppeteer to provide a clean interface for browser automation.
 * Manages browser lifecycle, pages, and provides common operations.
 */
import puppeteer from 'puppeteer';
export class BrowserController {
    browser = null;
    pages = new Map();
    consoleMessages = new Map();
    networkRequests = new Map();
    tabCounter = 0;
    headless;
    constructor(options = {}) {
        this.headless = options.headless ?? true;
    }
    /**
     * Initialize the browser
     */
    async init() {
        if (this.browser)
            return;
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
    async createTab() {
        if (!this.browser)
            await this.init();
        const page = await this.browser.newPage();
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
            if (messages.length > 1000)
                messages.shift();
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
            if (requests.length > 1000)
                requests.shift();
        });
        // Set reasonable viewport
        await page.setViewport({ width: 1280, height: 800 });
        this.pages.set(tabId, page);
        return tabId;
    }
    /**
     * Get all tabs info
     */
    async getTabs() {
        const tabs = [];
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
    getPage(tabId) {
        return this.pages.get(tabId);
    }
    /**
     * Navigate to URL
     */
    async navigate(tabId, url) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        await page.goto(url, { waitUntil: 'networkidle2', timeout: 30000 });
        return `Navigated to ${url}`;
    }
    /**
     * Get page text content
     */
    async getPageText(tabId) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        return await page.evaluate(() => document.body.innerText);
    }
    /**
     * Get accessibility tree (simplified)
     */
    async getAccessibilityTree(tabId, depth = 15) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        // Get accessibility snapshot
        const snapshot = await page.accessibility.snapshot({ root: undefined });
        if (!snapshot) {
            return 'No accessibility tree available';
        }
        // Format tree
        const formatNode = (node, indent = 0, refCounter = { count: 0 }) => {
            if (indent > depth)
                return '';
            const ref = `ref_${++refCounter.count}`;
            const prefix = '  '.repeat(indent);
            let result = `${prefix}${ref} ${node.role || 'unknown'}`;
            if (node.name)
                result += `: ${node.name}`;
            if (node.value)
                result += ` [value: ${node.value}]`;
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
    async findElements(tabId, query) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        const results = await page.evaluate((q) => {
            const matches = [];
            const queryLower = q.toLowerCase();
            const elements = Array.from(document.querySelectorAll('a, button, input, select, textarea, [role="button"], [role="link"], [onclick], [tabindex]'));
            for (const el of elements) {
                const text = (el.textContent || '').trim();
                const ariaLabel = el.getAttribute('aria-label') || '';
                const placeholder = el.placeholder || '';
                if (text.toLowerCase().includes(queryLower) ||
                    ariaLabel.toLowerCase().includes(queryLower) ||
                    placeholder.toLowerCase().includes(queryLower)) {
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
                if (matches.length >= 20)
                    break;
            }
            return matches;
        }, query);
        return results;
    }
    /**
     * Take screenshot
     */
    async screenshot(tabId) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        const viewport = page.viewport();
        const buffer = await page.screenshot({ type: 'jpeg', quality: 80 });
        return {
            data: Buffer.from(buffer).toString('base64'),
            width: viewport?.width || 1280,
            height: viewport?.height || 800,
        };
    }
    /**
     * Click at coordinates
     */
    async click(tabId, x, y, button = 'left', clickCount = 1) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        await page.mouse.click(x, y, { button, clickCount });
        return `Clicked at (${x}, ${y})`;
    }
    /**
     * Type text
     */
    async type(tabId, text) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        await page.keyboard.type(text);
        return `Typed: ${text}`;
    }
    /**
     * Press key
     */
    async pressKey(tabId, key, modifiers = []) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        // Build key combo
        let combo = key;
        if (modifiers.length > 0) {
            combo = [...modifiers, key].join('+');
        }
        await page.keyboard.press(combo);
        return `Pressed: ${combo}`;
    }
    /**
     * Scroll
     */
    async scroll(tabId, x, y, direction, amount = 3) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        const deltaX = direction === 'left' ? -100 * amount : direction === 'right' ? 100 * amount : 0;
        const deltaY = direction === 'up' ? -100 * amount : direction === 'down' ? 100 * amount : 0;
        await page.mouse.move(x, y);
        await page.mouse.wheel({ deltaX, deltaY });
        return `Scrolled ${direction} by ${amount}`;
    }
    /**
     * Execute JavaScript
     */
    async executeScript(tabId, script) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        return await page.evaluate(script);
    }
    /**
     * Set viewport size
     */
    async setViewport(tabId, width, height) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        await page.setViewport({ width, height });
        return `Viewport set to ${width}x${height}`;
    }
    /**
     * Get console messages
     */
    getConsoleMessages(tabId, limit = 100, onlyErrors = false) {
        let messages = this.consoleMessages.get(tabId) || [];
        if (onlyErrors) {
            messages = messages.filter(m => m.type === 'error');
        }
        return messages.slice(-limit);
    }
    /**
     * Get network requests
     */
    getNetworkRequests(tabId, limit = 100, urlPattern) {
        let requests = this.networkRequests.get(tabId) || [];
        if (urlPattern) {
            requests = requests.filter(r => r.url.includes(urlPattern));
        }
        return requests.slice(-limit);
    }
    /**
     * Fill form input
     */
    async fillInput(tabId, selector, value) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        await page.type(selector, value);
        return `Filled input with: ${value}`;
    }
    /**
     * Get page URL and title
     */
    async getPageInfo(tabId) {
        const page = this.pages.get(tabId);
        if (!page)
            throw new Error(`Tab not found: ${tabId}`);
        return {
            url: page.url(),
            title: await page.title(),
        };
    }
    /**
     * Close a tab
     */
    async closeTab(tabId) {
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
    async close() {
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
//# sourceMappingURL=browser-controller.js.map