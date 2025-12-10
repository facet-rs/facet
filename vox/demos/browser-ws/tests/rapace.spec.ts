import { test, expect } from "@playwright/test";

test.describe("rapace Explorer client", () => {
  test.beforeEach(async ({ page }) => {
    // Capture console messages for debugging
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        console.log(`BROWSER ERROR: ${msg.text()}`);
      }
    });

    page.on("pageerror", (err) => {
      console.log(`PAGE ERROR: ${err.message}`);
    });

    // Navigate to the test page
    await page.goto("/");

    // Wait for page to load
    await expect(page.locator("h1")).toContainText("rapace Explorer Demo");
  });

  test("should connect and discover services", async ({ page }) => {
    // Click connect
    await page.click("#connectBtn");

    // Wait for connection
    await expect(page.locator("#status")).toHaveText("Connected", { timeout: 10000 });

    // Check that services were discovered
    await expect(page.locator("#log")).toContainText("Connected!");
    await expect(page.locator("#log")).toContainText("Discovering services...");
    await expect(page.locator("#log")).toContainText("Found", { timeout: 5000 });

    // Services section should be visible
    await expect(page.locator("#services-container")).toBeVisible();

    // Should have at least one service card
    await expect(page.locator(".service-card")).toHaveCount(1, { timeout: 5000 });
  });

  test("should list service methods", async ({ page }) => {
    // Connect
    await page.click("#connectBtn");
    await expect(page.locator("#status")).toHaveText("Connected", { timeout: 10000 });

    // Wait for services to load
    await expect(page.locator(".service-card")).toHaveCount(1, { timeout: 5000 });

    // Click on the first service
    await page.locator(".service-card").first().click();

    // Methods section should appear
    await expect(page.locator("#methods-container")).toBeVisible();
    await expect(page.locator("#log")).toContainText("Loading methods");

    // Should have method items
    await expect(page.locator(".method-item")).not.toHaveCount(0, { timeout: 5000 });
  });

  test("should call a unary method", async ({ page }) => {
    // Connect
    await page.click("#connectBtn");
    await expect(page.locator("#status")).toHaveText("Connected", { timeout: 10000 });

    // Wait for services
    await expect(page.locator(".service-card")).toHaveCount(1, { timeout: 5000 });

    // Click Calculator service (or first service)
    await page.locator(".service-card").first().click();
    await expect(page.locator(".method-item")).not.toHaveCount(0, { timeout: 5000 });

    // Find the add method and call it
    const addMethod = page.locator(".method-item").filter({ hasText: "add" }).first();

    // Fill in arguments (a and b)
    await addMethod.locator("input").first().fill("7");
    await addMethod.locator("input").nth(1).fill("13");

    // Click the call button
    await addMethod.locator("button").click();

    // Check for result in log
    await expect(page.locator("#log")).toContainText("Result:", { timeout: 5000 });
    await expect(page.locator("#log")).toContainText("20");
  });

  test("should call a streaming method", async ({ page }) => {
    // Connect
    await page.click("#connectBtn");
    await expect(page.locator("#status")).toHaveText("Connected", { timeout: 10000 });

    // Wait for services
    await expect(page.locator(".service-card")).toHaveCount(1, { timeout: 5000 });

    // Click first service
    await page.locator(".service-card").first().click();
    await expect(page.locator(".method-item")).not.toHaveCount(0, { timeout: 5000 });

    // Find a streaming method (has streaming badge)
    const streamingMethod = page
      .locator(".method-item")
      .filter({ has: page.locator(".streaming-badge") })
      .first();

    // If there's a streaming method, test it
    const count = await streamingMethod.count();
    if (count > 0) {
      // Fill in argument
      await streamingMethod.locator("input").first().fill("3");

      // Click call
      await streamingMethod.locator("button").click();

      // Wait for stream items
      await expect(page.locator("#log")).toContainText("[0]", { timeout: 10000 });
      await expect(page.locator("#log")).toContainText("Stream complete");
    }
  });

  test("should disconnect cleanly", async ({ page }) => {
    // Connect
    await page.click("#connectBtn");
    await expect(page.locator("#status")).toHaveText("Connected", { timeout: 10000 });

    // Disconnect
    await page.click("#disconnectBtn");

    // Check status
    await expect(page.locator("#status")).toHaveText("Disconnected");
    await expect(page.locator("#log")).toContainText("Disconnected");

    // Services should be hidden
    await expect(page.locator("#services-container")).not.toBeVisible();
  });
});
