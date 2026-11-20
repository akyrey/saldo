import { expect, test } from "@playwright/test";
import { createAccount, login } from "./helpers";

// Phase 10 flows. Runs after flows.spec.ts (files run in name order), which
// did the setup; each flow reuses the cached session (login rate limit).

test("dark mode is applied before the first paint and survives a reload", async ({ page }) => {
  await login(page);
  // Record the theme the page had when the HTML was parsed, before React ran.
  await page.addInitScript(`
    document.addEventListener("DOMContentLoaded", () => {
      window.darkAtLoad = document.documentElement.classList.contains("dark");
    });
  `);
  await page.getByRole("button", { name: "More" }).first().click();
  await page.getByRole("radio", { name: "Dark" }).first().click();
  await expect(page.locator("html")).toHaveClass(/dark/);

  await page.reload();
  await expect(page.getByRole("heading", { name: "Accounts", exact: true })).toBeVisible();
  expect(await page.evaluate("window.darkAtLoad")).toBe(true);
  await expect(page.locator("html")).toHaveClass(/dark/);

  // Back to following the system, for the flows after this one.
  await page.getByRole("button", { name: "More" }).first().click();
  await page.getByRole("radio", { name: "System" }).first().click();
});

test("filter by amount, save the filters as a view, and restore them from its chip", async ({
  page,
}) => {
  await login(page);
  const bank = await createAccount(page, "View bank", "asset");
  const shop = await createAccount(page, "View shop", "expense");
  const origin = new URL(page.url()).origin;
  for (const amount of ["12.00", "250.00"]) {
    const response = await page.request.post("/api/v1/transactions", {
      headers: { "X-Saldo-Csrf": "1", Origin: origin },
      data: {
        splits: [
          {
            date: "2026-10-03",
            description: `View purchase ${amount}`,
            source_id: bank,
            destination_id: shop,
            amount,
          },
        ],
      },
    });
    expect(response.status(), await response.text()).toBe(201);
  }

  await page.goto("/transactions?q=View%20purchase");
  await page.getByRole("button", { name: /Filters/ }).click();
  const from = page.getByLabel("Amount from");
  await from.fill("100");
  await from.press("Enter");
  await expect(page).toHaveURL(/amount_min=/);
  await expect(page.getByText("View purchase 250.00")).toBeVisible();
  await expect(page.getByText("View purchase 12.00")).toHaveCount(0);

  await page.getByRole("button", { name: /Save view/ }).click();
  await page.getByLabel("View name").fill("Big view purchases");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const chip = page.getByRole("button", { name: "Big view purchases", exact: true });
  await expect(chip).toHaveAttribute("aria-pressed", "true");

  await page.goto("/transactions");
  await expect(chip).toHaveAttribute("aria-pressed", "false");
  await chip.click();
  await expect(page).toHaveURL(/amount_min=/);
  await expect(page).toHaveURL(/q=View/);
  await expect(page.getByText("View purchase 250.00")).toBeVisible();
  await expect(page.getByText("View purchase 12.00")).toHaveCount(0);
});

test("the command palette and shortcuts move around the app", async ({ page }) => {
  await login(page);
  await page.keyboard.press("ControlOrMeta+k");
  const input = page.getByPlaceholder(/Go to a page/);
  await expect(input).toBeVisible();
  await input.fill("budg");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Budgets", exact: true })).toBeVisible();

  await page.keyboard.press("n");
  await expect(page).toHaveURL(/\/transactions\/new$/);
  await page.keyboard.press("Escape");
  await page.locator("body").click();
  await page.keyboard.press("g");
  await page.keyboard.press("t");
  await expect(page.getByRole("heading", { name: "Transactions", exact: true })).toBeVisible();
});

test("a net worth goal shows on the dashboard next to safe to spend", async ({ page }) => {
  await login(page);
  await page.goto("/reports/net-worth");
  await page.getByLabel("Name").fill("Rainy day fund");
  await page.getByLabel("Target").fill("10000");
  await page.getByRole("button", { name: "Add goal" }).click();
  await expect(page.getByTestId("goal").filter({ hasText: "Rainy day fund" })).toBeVisible();

  await page.goto("/reports");
  await expect(page.getByTestId("safe-to-spend")).toBeVisible();
  const goal = page.getByTestId("goal").filter({ hasText: "Rainy day fund" });
  await expect(goal).toBeVisible();
  await expect(goal.getByRole("progressbar")).toBeVisible();
});
