import { defineConfig, devices } from "@playwright/test";

const port = process.env.E2E_PORT ?? "8099";
const baseURL = `http://127.0.0.1:${port}`;

// The flows share one fresh database (setup can only run once), so they run
// in order in a single worker.
export default defineConfig({
  testDir: "e2e",
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL,
    locale: "en-US",
    trace: "retain-on-failure",
    // Requests must reach the server, not a cached shell.
    serviceWorkers: "block",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "node e2e/start-server.mjs",
    url: `${baseURL}/healthz`,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
