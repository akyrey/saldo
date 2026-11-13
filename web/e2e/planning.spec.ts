// Budgets and reports. Runs after flows.spec.ts, which sets up the household.
import { expect, test } from "@playwright/test";
import { createAccount, login } from "./helpers";

test("a budget shows what was spent and appears in the reports", async ({ page }) => {
  await login(page);
  const wallet = await createAccount(page, "Wallet", "asset");
  const market = await createAccount(page, "Market", "expense");

  await page.getByRole("link", { name: "Budgets" }).first().click();
  await expect(page.getByRole("heading", { name: "Budgets" })).toBeVisible();
  const form = page.getByRole("form", { name: "New budget" });
  await form.getByLabel("Name").fill("Food");
  await form.getByLabel("Automatic limit").selectOption("reset");
  await form.getByLabel("Amount").fill("300");
  await form.getByRole("button", { name: "Add budget" }).click();
  await expect(page.getByRole("progressbar", { name: "Spent 0.00 of 300.00 EUR" })).toBeVisible();

  // A withdrawal booked on the budget today.
  const budgets = await page.request.get("/api/v1/budgets");
  const budget = ((await budgets.json()) as { data: { id: string; name: string }[] }).data.find(
    (b) => b.name === "Food",
  );
  const today = await page.evaluate(() => {
    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  });
  const origin = new URL(page.url()).origin;
  const created = await page.request.post("/api/v1/transactions", {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data: {
      splits: [
        {
          date: today,
          description: "Vegetables",
          source_id: wallet,
          destination_id: market,
          amount: "45.50",
          budget_id: budget?.id,
        },
      ],
    },
  });
  expect(created.status(), await created.text()).toBe(201);

  await page.reload();
  await expect(page.getByRole("progressbar", { name: "Spent 45.50 of 300.00 EUR" })).toBeVisible();

  await page.getByRole("link", { name: "Food" }).click();
  const add = page.getByRole("form", { name: "Add limit" });
  await add.getByLabel("From").fill("2030-01-01");
  await add.getByLabel("To").fill("2030-01-31");
  await add.getByLabel("Limit").fill("120");
  await add.getByRole("button", { name: "Add limit" }).click();
  await expect(page.getByRole("progressbar", { name: "Spent 0.00 of 120.00 EUR" })).toBeVisible();

  await page.goto("/reports/budget-vs-actual");
  const table = page.getByRole("table");
  await expect(table.getByRole("row", { name: /Food/ })).toContainText("45.50");
  await page.goto("/reports");
  await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
  await expect(page.getByTestId("net-worth-now")).toBeVisible();
});
