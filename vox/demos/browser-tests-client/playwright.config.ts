import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  fullyParallel: false, // Run tests sequentially - we have one server
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? 'github' : 'list',
  timeout: 30000,
  use: {
    baseURL: 'http://127.0.0.1:8088',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        launchOptions: {
          // Allow localhost connections from sandboxed browser
          args: [
            '--disable-web-security',
            '--allow-running-insecure-content',
            '--disable-features=IsolateOrigins,site-per-process',
          ],
        },
      },
    },
  ],
  // Web server is managed externally by xtask
});
