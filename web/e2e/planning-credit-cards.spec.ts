// Phase 12: a credit card's monthly plan, what is left, the automatic
// payment and moving a purchase to the next month, against the real server.
// Runs after flows.spec.ts, which sets up the household.
import { expect, test, type Page } from "@playwright/test";
import { createAccount, login } from "./helpers";

async function post(page: Page, path: string, data: object): Promise<string> {
  const origin = new URL(page.url()).origin;
  const response = await page.request.post(path, {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data,
  });
  expect(response.status(), await response.text()).toBe(201);
  return ((await response.json()) as { data: { id: string } }).data.id;
}

/** The ISO date of `day` in the current month (UTC, the test household's zone). */
function thisMonth(day: number): string {
  const now = new Date();
  return new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), day))
    .toISOString()
    .slice(0, 10);
}

/** The 15th of next month, when this month is paid. */
function paymentDate(): string {
  const now = new Date();
  return new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 15))
    .toISOString()
    .slice(0, 10);
}

test("a card plan tracks what is left and keeps the payment in sync", async ({ page }) => {
  await login(page);
  await createAccount(page, "Card bank", "asset");
  const shop = await createAccount(page, "Card shop", "expense");
  const card = await post(page, "/api/v1/accounts", {
    name: "E2E Visa",
    type: "asset",
    asset_role: "credit_card",
  });

  await page.goto(`/accounts/${card}`);
  const panel = page.getByTestId("credit-card-panel");
  await panel.getByRole("button", { name: "Set up a monthly plan" }).click();
  await panel.getByLabel("Monthly budget (EUR)").fill("500.00");
  await panel.getByLabel("Paid from").selectOption({ label: "Card bank" });
  await panel.getByLabel("Payment day of the next month").fill("15");
  await panel.getByRole("button", { name: "Set up plan" }).click();
  await expect(panel.getByRole("button", { name: "Edit plan" })).toBeVisible();

  const buy = (day: number, amount: string, description: string) =>
    post(page, "/api/v1/transactions", {
      splits: [
        { date: thisMonth(day), description, source_id: card, destination_id: shop, amount },
      ],
    });
  await buy(1, "100.00", "Card groceries");
  await buy(2, "50.00", "Card dinner");

  const available = panel.getByTestId("card-available");
  const settlement = panel.getByTestId("card-settlement");
  // The payment is booked by a background task right after the writes.
  await expect(async () => {
    await page.reload();
    await expect(available).toHaveText("€350.00");
    await expect(settlement).toContainText(`€150.00 from Card bank on ${paymentDate()}`);
  }).toPass({ timeout: 15_000 });

  const dinner = panel.getByRole("listitem").filter({ hasText: "Card dinner" });
  await dinner.getByRole("button", { name: "Count in next month" }).click();
  await expect(available).toHaveText("€400.00");
  await expect(dinner).toContainText("to ");
  await expect(async () => {
    await page.reload();
    await expect(settlement).toContainText("€100.00 from Card bank");
  }).toPass({ timeout: 15_000 });
});
