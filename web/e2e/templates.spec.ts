import { expect, test } from "@playwright/test";
import { createAccount, login } from "./helpers";

// Runs after flows.spec.ts (files run in name order), which did the setup.
// One login only: the suite shares the login rate limit (10 per 15 minutes).
test("save a transaction as a template, use it, and duplicate a transaction", async ({ page }) => {
  await login(page);
  const bank = await createAccount(page, "Tpl bank", "asset");
  const shop = await createAccount(page, "Tpl bakery", "expense");
  const origin = new URL(page.url()).origin;
  const created = await page.request.post("/api/v1/transactions", {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data: {
      splits: [
        {
          date: "2026-10-01",
          description: "Bread run",
          source_id: bank,
          destination_id: shop,
          amount: "4.20",
        },
      ],
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  const groupId = ((await created.json()) as { data: { id: string } }).data.id;

  // Save as template from the existing transaction.
  await page.goto(`/transactions/${groupId}/edit`);
  await page.getByRole("button", { name: "Save as template" }).click();
  await page.getByLabel("Template name").fill("Bakery");
  await page.getByRole("button", { name: "Save template" }).click();
  await expect(page.getByRole("status")).toHaveText("Template saved");

  // Use it with a new amount.
  await page.goto("/transactions");
  await page.getByLabel("New from template").selectOption({ label: "Bakery" });
  await expect(page.getByRole("heading", { name: "New from “Bakery”" })).toBeVisible();
  const split = page.getByRole("group", { name: "Split 1" });
  await expect(split.getByLabel("Description")).toHaveValue("Bread run");
  await split.getByLabel(/^Amount/).fill("6.80");
  await page.getByRole("button", { name: "Create transaction" }).click();
  await expect(page).toHaveURL(/\/transactions$/);

  // Duplicate the original: same amount, today's date.
  await page.goto(`/transactions/${groupId}/edit`);
  await page.getByRole("link", { name: /Duplicate/ }).click();
  await expect(page.getByRole("heading", { name: "Duplicate transaction" })).toBeVisible();
  await page.getByRole("button", { name: "Create transaction" }).click();
  await expect(page).toHaveURL(/\/transactions$/);

  await page.goto("/transactions?q=Bread%20run");
  const rows = page.getByRole("row", { name: /Tpl bank → Tpl bakery/ });
  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: "6.80" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "4.20" })).toHaveCount(2);

  await page.goto("/templates");
  await expect(page.getByRole("listitem", { name: "Bakery" })).toContainText(
    "Bread run · 4.20 EUR",
  );
});
