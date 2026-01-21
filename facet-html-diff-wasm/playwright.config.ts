import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "list",
  use: {
    baseURL: "http://localhost:3456",
  },
  webServer: {
    command: "pnpm exec serve -l 3456",
    url: "http://localhost:3456",
    reuseExistingServer: !process.env.CI,
    timeout: 5000,
  },
});
