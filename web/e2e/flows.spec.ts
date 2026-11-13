import { expect, test, type Page } from "@playwright/test";
import { createAccount, login, OWNER } from "./helpers";

test.describe.configure({ mode: "serial" });

function split(page: Page, number: number) {
  return page.getByRole("group", { name: `Split ${number}` });
}

async function fillSplit(
  page: Page,
  number: number,
  values: { description: string; source: string; destination: string; amount: string },
) {
  const row = split(page, number);
  await row.getByLabel("Description").fill(values.description);
  await row.getByLabel("Source account").selectOption({ label: `${values.source} (EUR)` });
  await row
    .getByLabel("Destination account")
    .selectOption({ label: `${values.destination} (EUR)` });
  await row.getByLabel(/^Amount/).fill(values.amount);
}

test("first-run setup, logout and login", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL(/\/setup$/);
  await page.getByLabel("Your name").fill(OWNER.name);
  await page.getByLabel("Email").fill(OWNER.email);
  await page.getByLabel(/^Password/).fill(OWNER.password);
  await page.getByLabel("Household name").fill(OWNER.household);
  await page.getByRole("button", { name: "Create household" }).click();
  await expect(page.getByRole("heading", { name: "Accounts", exact: true })).toBeVisible();
  await expect(page.getByText(`${OWNER.name} · ${OWNER.household}`)).toBeVisible();

  await page.getByRole("button", { name: "Log out" }).click();
  await expect(page).toHaveURL(/\/login/);
  await page.goto("/transactions");
  await expect(page).toHaveURL(/\/login\?redirect=/);

  await page.getByLabel("Email").fill(OWNER.email);
  await page.getByLabel("Password").fill("wrong password");
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page.getByRole("alert")).toBeVisible();

  await page.getByLabel("Password").fill(OWNER.password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page.getByRole("heading", { name: "Transactions" })).toBeVisible();
});

test("a three-split transaction with different accounts changes three balances", async ({
  page,
}) => {
  await login(page);
  await createAccount(page, "Checking", "asset");
  await createAccount(page, "Credit card", "liability");
  await createAccount(page, "Savings", "asset");
  await createAccount(page, "Groceries", "expense");
  await createAccount(page, "Salary", "revenue");

  // A full load, so the app sees the accounts created behind its back.
  await page.goto("/transactions");
  await page.getByRole("link", { name: "New transaction" }).click();
  await page.getByLabel(/^Title/).fill("Saturday");
  await fillSplit(page, 1, {
    description: "Bread and milk",
    source: "Checking",
    destination: "Groceries",
    amount: "42.50",
  });
  await expect(split(page, 1).getByText("Withdrawal")).toBeVisible();
  await page.getByRole("button", { name: "Add split" }).click();
  await fillSplit(page, 2, {
    description: "Wine",
    source: "Credit card",
    destination: "Groceries",
    amount: "17,25",
  });
  await page.getByRole("button", { name: "Add split" }).click();
  await fillSplit(page, 3, {
    description: "Bonus",
    source: "Salary",
    destination: "Savings",
    amount: "100",
  });
  await expect(split(page, 3).getByText("Deposit")).toBeVisible();
  await expect(page.getByTestId("editor-total")).toHaveText("Total: 159.75 EUR");
  await page.getByRole("button", { name: "Create transaction" }).click();

  await expect(page.getByRole("heading", { name: "Transactions" })).toBeVisible();
  await expect(page.getByText("Saturday")).toBeVisible();
  await expect(page.getByText("(3 splits)")).toBeVisible();

  await page.getByRole("link", { name: "Accounts" }).first().click();
  await expect(page.getByTestId("balance-Checking")).toHaveText("-€42.50");
  await expect(page.getByTestId("balance-Credit card")).toHaveText("-€17.25");
  await expect(page.getByTestId("balance-Savings")).toHaveText("€100.00");
  await expect(page.getByTestId("balance-Groceries")).toHaveText("€59.75");
});

test("a server validation error is shown on the right split row", async ({ page }) => {
  await login(page);
  await page.goto("/transactions/new");
  await fillSplit(page, 1, {
    description: "Fine",
    source: "Checking",
    destination: "Groceries",
    amount: "5.00",
  });
  await page.getByRole("button", { name: "Add split" }).click();
  await fillSplit(page, 2, {
    description: "Zero",
    source: "Checking",
    destination: "Groceries",
    amount: "0",
  });
  await page.getByRole("button", { name: "Create transaction" }).click();

  const second = split(page, 2);
  await expect(second.getByTestId("split-1-amount-error")).toBeVisible();
  await expect(second.getByLabel(/^Amount/)).toHaveAttribute("aria-invalid", "true");
  await expect(split(page, 1).getByTestId("split-0-amount-error")).toHaveCount(0);
  await expect(page).toHaveURL(/\/transactions\/new$/);

  await page.goto("/accounts");
  await expect(page.getByTestId("balance-Checking")).toHaveText("-€42.50");
});

test("the PWA manifest and service worker are served", async ({ request }) => {
  const manifest = await request.get("/manifest.webmanifest");
  expect(manifest.ok()).toBe(true);
  const body = (await manifest.json()) as { name: string; icons: { sizes: string }[] };
  expect(body.name).toBe("Saldo");
  expect(body.icons.map((icon) => icon.sizes)).toContain("512x512");
  const worker = await request.get("/sw.js");
  expect(worker.ok()).toBe(true);
  expect(worker.headers()["cache-control"]).toBe("no-cache");
  const index = await (await request.get("/")).text();
  expect(index).toContain("manifest.webmanifest");
});
