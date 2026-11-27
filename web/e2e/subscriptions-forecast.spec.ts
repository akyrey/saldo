// Phase 9 flows against the real server: accepting a subscription
// suggestion, and the cash-flow forecast's horizon. Runs after
// flows.spec.ts, which sets up the household. (The statement import flow
// is in statements.spec.ts.)
import { expect, test, type Page } from "@playwright/test";
import { createAccount, login } from "./helpers";

async function post(page: Page, path: string, data: object): Promise<void> {
  const origin = new URL(page.url()).origin;
  const response = await page.request.post(path, {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data,
  });
  expect(response.status(), await response.text()).toBe(201);
}

/** The ISO date `offset` months from today, on `day` (UTC). */
function monthsFromNow(offset: number, day: number): string {
  const now = new Date();
  return new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + offset, day))
    .toISOString()
    .slice(0, 10);
}

async function withdrawal(page: Page, from: string, to: string, date: string, text: string) {
  await post(page, "/api/v1/transactions", {
    splits: [{ date, description: text, source_id: from, destination_id: to, amount: "9.99" }],
  });
}

test("a repeating payment is suggested and accepted as a bill", async ({ page }) => {
  await login(page);
  const bank = await createAccount(page, "Suggestion bank", "asset");
  const service = await createAccount(page, "Streamly", "expense");
  // Three monthly payments, the last one a month ago: a monthly pattern
  // whose next payment is due about now. Day 28 or earlier exists in every month.
  const day = Math.min(new Date().getUTCDate(), 28);
  for (const offset of [-3, -2, -1]) {
    await withdrawal(page, bank, service, monthsFromNow(offset, day), "Streamly monthly");
  }

  await page.goto("/bills");
  const suggestion = page.getByRole("listitem").filter({ hasText: "Streamly" });
  await expect(suggestion).toContainText("3 payments");
  await suggestion.getByRole("button", { name: "Add as bill" }).click();
  const form = page.getByRole("form", { name: "Add Streamly as a bill" });
  await form.getByLabel("Bill name").fill("Streamly subscription");
  await form.getByRole("button", { name: "Create bill" }).click();

  // The suggestion is gone and the bill exists, with the payments linked.
  await expect(page.getByRole("button", { name: "Dismiss Streamly" })).toHaveCount(0);
  await expect(page.getByText("Streamly subscription")).toBeVisible();
  const origin = new URL(page.url()).origin;
  const candidates = await page.request.get("/api/v1/subscription-candidates", {
    headers: { Origin: origin },
  });
  const body = (await candidates.json()) as { data: { account_name: string }[] };
  expect(body.data.map((c) => c.account_name)).not.toContain("Streamly");
});

test("the forecast shows planned items within the chosen horizon", async ({ page }) => {
  await login(page);
  const bank = await createAccount(page, "Forecast bank", "asset");
  const garage = await createAccount(page, "Forecast garage", "expense");
  // Five months ahead: outside the default three months, inside a year.
  await withdrawal(page, bank, garage, monthsFromNow(5, 15), "Planned car service");

  await page.goto("/forecast");
  const horizon = page.getByRole("group", { name: "Horizon" });
  await expect(horizon.getByRole("button", { name: "3 mo" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  // Wait for the forecast itself, so "not shown" is not just "not loaded yet".
  await expect(page.getByRole("region", { name: "Forecast in EUR" })).toBeVisible();
  await expect(page.getByText("Planned car service")).toHaveCount(0);

  await horizon.getByRole("button", { name: "1 yr" }).click();
  await expect(page).toHaveURL(/months=12/);
  await expect(horizon.getByRole("button", { name: "1 yr" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByText("Planned car service")).toBeVisible();

  // The horizon survives a reload (it lives in the URL).
  await page.reload();
  await expect(page.getByText("Planned car service")).toBeVisible();
});
