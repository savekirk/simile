import { defineConfig } from "@playwright/test"
const deployedURL = process.env.PLAYGROUND_BASE_URL
export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 60_000,
  expect: { timeout: 20_000 },
  workers: 1,
  use: {
    baseURL: deployedURL ?? "http://127.0.0.1:4173",
    headless: true,
    viewport: { width: 1440, height: 1000 },
    trace: "retain-on-failure",
    launchOptions: process.env.CHROMIUM_PATH
      ? { executablePath: process.env.CHROMIUM_PATH }
      : undefined,
  },
  webServer: deployedURL
    ? undefined
    : {
        command: "pnpm exec vite preview --host 127.0.0.1 --port 4173",
        port: 4173,
        reuseExistingServer: !process.env.CI,
      },
})
