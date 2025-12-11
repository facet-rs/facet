import { test, expect, Page } from "@playwright/test";

async function connectToService(page: Page) {
  await page.click("#connectBtn");
  await expect(page.locator("#status")).toHaveText("Connected", { timeout: 10000 });
}

test.describe("BrowserDemo harness", () => {
  test.beforeEach(async ({ page }) => {
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        console.log(`BROWSER ERROR: ${msg.text()}`);
      }
    });

    page.on("pageerror", (err) => {
      console.log(`PAGE ERROR: ${err.message}`);
    });

    await page.goto("/");
    await expect(page.locator("h1")).toContainText("rapace Browser Demo");
  });

  test("connects to the BrowserDemo service", async ({ page }) => {
    await connectToService(page);
    await expect(page.locator("#log")).toContainText("BrowserDemo service", { timeout: 5000 });
  });

  test("summarizes numbers over RPC", async ({ page }) => {
    await connectToService(page);

    await page.fill("#numbersInput", "5, 10, 15");
    await page.click("#numbersBtn");

    await expect(page.locator("#numbersResult")).toContainText("sum=30", { timeout: 5000 });
    await expect(page.locator("#log")).toContainText("Summary result", { timeout: 5000 });
  });

  test("transforms phrases", async ({ page }) => {
    await connectToService(page);

    await page.fill("#phraseInput", "hello from rapace");
    await page.check("#shoutToggle");
    await page.click("#phraseBtn");

    await expect(page.locator("#phraseResult")).toContainText("HELLO FROM RAPACE", { timeout: 5000 });
  });

  test("streams countdown events", async ({ page }) => {
    await connectToService(page);

    await page.fill("#countdownStart", "3");
    await page.click("#countdownBtn");

    const items = page.locator("#countdownList li");
    await expect(items).toHaveCount(4, { timeout: 10000 });
    await expect(items.last()).toContainText("value=0", { timeout: 10000 });
    await expect(page.locator("#log")).toContainText("Countdown complete", { timeout: 10000 });
  });

  test("disconnects cleanly", async ({ page }) => {
    await connectToService(page);

    await page.click("#disconnectBtn");
    await expect(page.locator("#status")).toHaveText("Disconnected");
    await expect(page.locator("#log")).toContainText("Disconnected");
  });
});
