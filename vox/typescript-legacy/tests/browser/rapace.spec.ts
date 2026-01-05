import { test, expect } from "@playwright/test";

test.describe("Rapace in browser", () => {
  test("runs all browser tests", async ({ page }) => {
    // Navigate to the test page
    await page.goto("/tests/browser/test-page.html");

    // Wait for tests to complete (max 30 seconds)
    await page.waitForFunction(
      () => (window as unknown as { testResults?: { total: number } }).testResults !== undefined,
      { timeout: 30000 }
    );

    // Get test results
    const results = await page.evaluate(
      () => (window as unknown as { testResults: { passed: number; failed: number; total: number } }).testResults
    );

    console.log(`Browser tests: ${results.passed}/${results.total} passed`);

    // Check results
    expect(results.failed).toBe(0);
    expect(results.passed).toBeGreaterThan(0);
  });

  test("postcard encoding works in browser", async ({ page }) => {
    await page.goto("/tests/browser/test-page.html");

    // Run a specific encoding test
    const result = await page.evaluate(async () => {
      const { PostcardEncoder, PostcardDecoder } = await import("/dist/index.js");

      const encoder = new PostcardEncoder();
      encoder.u64(12345n);
      encoder.string("test");
      encoder.bool(true);

      const decoder = new PostcardDecoder(encoder.bytes);
      return {
        u64: decoder.u64().toString(),
        string: decoder.string(),
        bool: decoder.bool(),
      };
    });

    expect(result.u64).toBe("12345");
    expect(result.string).toBe("test");
    expect(result.bool).toBe(true);
  });

  test("method ID computation is consistent with Node.js", async ({ page }) => {
    await page.goto("/tests/browser/test-page.html");

    const browserMethodId = await page.evaluate(async () => {
      const { computeMethodId } = await import("/dist/index.js");
      return computeMethodId("BrowserDemo", "summarize_numbers");
    });

    // Import and compute in Node.js
    const { computeMethodId } = await import("../../dist/index.js");
    const nodeMethodId = computeMethodId("BrowserDemo", "summarize_numbers");

    expect(browserMethodId).toBe(nodeMethodId);
  });
});
