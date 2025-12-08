import { test, expect } from '@playwright/test';

test.describe('rapace WebSocket client', () => {
  test.beforeEach(async ({ page }) => {
    // Capture console messages for debugging
    page.on('console', msg => {
      if (msg.type() === 'error') {
        console.log(`BROWSER ERROR: ${msg.text()}`);
      }
    });

    page.on('pageerror', err => {
      console.log(`PAGE ERROR: ${err.message}`);
    });

    // Navigate to the test page
    await page.goto('/');

    // Wait for page to load
    await expect(page.locator('h1')).toContainText('rapace WebSocket Client Demo');
  });

  test('should connect to WebSocket server', async ({ page }) => {
    // Click connect
    await page.click('#connectBtn');

    // Wait for connection
    await expect(page.locator('#status')).toHaveText('Connected', { timeout: 10000 });

    // Check log for success message
    await expect(page.locator('#log')).toContainText('Connected!');
  });

  test('should call Adder service', async ({ page }) => {
    // Connect first
    await page.click('#connectBtn');
    await expect(page.locator('#status')).toHaveText('Connected', { timeout: 10000 });

    // Set values
    await page.fill('#adderA', '7');
    await page.fill('#adderB', '13');

    // Call adder
    await page.click('#adderBtn');

    // Wait for result
    await expect(page.locator('#log')).toContainText('7 + 13 = 20', { timeout: 5000 });
  });

  test('should stream from Range service', async ({ page }) => {
    // Connect first
    await page.click('#connectBtn');
    await expect(page.locator('#status')).toHaveText('Connected', { timeout: 10000 });

    // Set range value
    await page.fill('#rangeN', '5');

    // Call range
    await page.click('#rangeBtn');

    // Wait for stream to complete - check for all items
    const log = page.locator('#log');
    await expect(log).toContainText('Stream item 0: 0', { timeout: 10000 });
    await expect(log).toContainText('Stream item 1: 1');
    await expect(log).toContainText('Stream item 2: 2');
    await expect(log).toContainText('Stream item 3: 3');
    await expect(log).toContainText('Stream item 4: 4');
    await expect(log).toContainText('Stream complete, received 5 items');
  });

  test('should handle multiple sequential calls', async ({ page }) => {
    // Connect
    await page.click('#connectBtn');
    await expect(page.locator('#status')).toHaveText('Connected', { timeout: 10000 });

    // First call
    await page.fill('#adderA', '1');
    await page.fill('#adderB', '2');
    await page.click('#adderBtn');
    await expect(page.locator('#log')).toContainText('1 + 2 = 3', { timeout: 5000 });

    // Second call
    await page.fill('#adderA', '100');
    await page.fill('#adderB', '200');
    await page.click('#adderBtn');
    await expect(page.locator('#log')).toContainText('100 + 200 = 300', { timeout: 5000 });
  });

  test('should disconnect cleanly', async ({ page }) => {
    // Connect
    await page.click('#connectBtn');
    await expect(page.locator('#status')).toHaveText('Connected', { timeout: 10000 });

    // Disconnect
    await page.click('#disconnectBtn');

    // Check status
    await expect(page.locator('#status')).toHaveText('Disconnected');
    await expect(page.locator('#log')).toContainText('Disconnected');
  });
});
