import { expect, test } from "@playwright/test";
import { login } from "./helpers";

// The server sends a strict Content-Security-Policy (no inline scripts).
// Runs after flows.spec.ts, which did the setup. Opens the main pages,
// including the chart-heavy ones, and fails on any blocked resource.

const PAGES = [
  "/accounts",
  "/transactions",
  "/transactions/new",
  "/budgets",
  "/bills",
  "/reports",
  "/forecast",
  "/categories",
  "/tags",
  "/settings/notifications",
];

test("the main pages load without a Content-Security-Policy violation", async ({ page }) => {
  const violations: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().includes("Content Security Policy")) {
      violations.push(message.text());
    }
  });

  const response = await page.goto("/login");
  expect(response?.headers()["content-security-policy"]).toContain("script-src 'self'");
  expect(response?.headers()["x-frame-options"]).toBe("DENY");

  await login(page);
  for (const path of PAGES) {
    await page.goto(path);
    await page.waitForLoadState("networkidle");
  }
  expect(violations).toEqual([]);
});
