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

test("a second household member sees the owner's transaction", async ({ page }) => {
  await login(page);
  const origin = new URL(page.url()).origin;
  const member = { email: "bo@example.com", display_name: "Bo", password: "another long password" };
  const added = await page.request.post("/api/v1/household/members", {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data: member,
  });
  expect(added.status(), await added.text()).toBe(201);
  await page.getByRole("button", { name: "Log out" }).click();

  await login(page, member.email, member.password);
  await expect(page.getByTestId("balance-Checking")).toHaveText("-€42.50");
  await page.getByRole("link", { name: "Transactions" }).first().click();
  await expect(page.getByText("Saturday")).toBeVisible();
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

test("the owner downloads the JSON and CSV exports", async ({ page }) => {
  await login(page);
  await page.getByRole("link", { name: "Export" }).first().click();
  await expect(page.getByRole("heading", { name: "Export" })).toBeVisible();

  const [json] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Download JSON" }).click(),
  ]);
  expect(json.suggestedFilename()).toMatch(/^saldo-export-\d{4}-\d{2}-\d{2}\.json$/);
  const path = await json.path();
  const { readFile } = await import("node:fs/promises");
  const document = JSON.parse(await readFile(path, "utf8")) as {
    format: string;
    accounts: unknown[];
  };
  expect(document.format).toBe("saldo-export");
  expect(document.accounts.length).toBeGreaterThan(0);

  const [csv] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Download CSV" }).click(),
  ]);
  expect(csv.suggestedFilename()).toMatch(/\.zip$/);
});

test("a bill is created and a period is marked paid with a transaction", async ({ page }) => {
  await login(page);
  const checking = await createAccount(page, "Bills checking", "asset");
  const landlord = await createAccount(page, "Bills landlord", "expense");
  const today = new Date().toISOString().slice(0, 10);
  const origin = new URL(page.url()).origin;
  const payment = await page.request.post("/api/v1/transactions", {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data: {
      splits: [
        {
          date: today,
          description: "Flat rent",
          source_id: checking,
          destination_id: landlord,
          amount: "750",
        },
      ],
    },
  });
  expect(payment.status(), await payment.text()).toBe(201);

  await page.goto("/bills/new");
  await page.getByLabel("Name").fill("Flat rent");
  await page.getByLabel("Minimum amount").fill("750");
  await page.getByLabel("Maximum amount").fill("750");
  await page.getByLabel("First due date").fill(today);
  await page.getByRole("button", { name: "Create bill" }).click();
  await expect(page.getByRole("heading", { name: "Flat rent" })).toBeVisible();

  const row = page.getByRole("row").filter({ hasText: today });
  await row.getByRole("button", { name: "Mark paid" }).click();
  const picker = row.getByLabel("Paying transaction");
  const option = picker.locator("option", { hasText: "Flat rent" });
  await picker.selectOption((await option.getAttribute("value")) ?? "");
  await row.getByRole("button", { name: "Confirm payment" }).click();
  await expect(row.getByText("paid", { exact: true })).toBeVisible();

  await page.goto("/bills");
  await expect(page.getByRole("link", { name: "Flat rent" })).toBeVisible();
});

test("a rule built in the editor categorises a new transaction", async ({ page }) => {
  await login(page);
  const checking = await createAccount(page, "Rules checking", "asset");
  const shop = await createAccount(page, "Rules bakery", "expense");
  const origin = new URL(page.url()).origin;
  const headers = { "X-Saldo-Csrf": "1", Origin: origin };
  const category = await page.request.post("/api/v1/categories", {
    headers,
    data: { name: "Bread" },
  });
  expect(category.status()).toBe(201);

  await page.goto("/rules");
  await page.getByLabel("New group title").fill("Everyday");
  await page.getByRole("button", { name: "Add group" }).click();
  const group = page.getByRole("region", { name: "Everyday" });
  await group.getByRole("link", { name: "New rule" }).click();
  await page.getByLabel("Title", { exact: true }).fill("Bakery to bread");
  await page.getByLabel("Trigger 1 value").fill("bakery");
  await page.getByLabel("Action 1 value").selectOption({ label: "Bread" });
  await page.getByRole("button", { name: "Create rule" }).click();
  await expect(page.getByRole("button", { name: "Test rule" })).toBeVisible();

  const created = await page.request.post("/api/v1/transactions", {
    headers,
    data: {
      splits: [
        {
          date: new Date().toISOString().slice(0, 10),
          description: "Morning bakery",
          source_id: checking,
          destination_id: shop,
          amount: "3.20",
        },
      ],
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  const body = (await created.json()) as { data: { splits: { category_id: string | null }[] } };
  const bread = ((await category.json()) as { data: { id: string } }).data.id;
  expect(body.data.splits[0]?.category_id).toBe(bread);
});

test("a transaction by one member appears in the other's open tab; the owner adds a webhook", async ({
  browser,
}) => {
  // Two browser contexts are two separate browsers: two sessions, two streams.
  const ownerContext = await browser.newContext();
  const memberContext = await browser.newContext();
  const owner = await ownerContext.newPage();
  const member = await memberContext.newPage();
  await login(owner);
  await login(member, "bo@example.com", "another long password");
  await member.getByRole("link", { name: "Transactions" }).first().click();
  await expect(member.getByRole("heading", { name: "Transactions" })).toBeVisible();
  await expect(member.getByText("Live coffee")).toHaveCount(0);

  const source = await createAccount(owner, "Live wallet", "asset");
  const destination = await createAccount(owner, "Live café", "expense");
  const created = await owner.request.post("/api/v1/transactions", {
    headers: { "X-Saldo-Csrf": "1", Origin: new URL(owner.url()).origin },
    data: {
      title: "Live coffee",
      splits: [
        {
          date: "2026-11-07",
          description: "Live coffee",
          source_id: source,
          destination_id: destination,
          amount: "2.40",
        },
      ],
    },
  });
  expect(created.status(), await created.text()).toBe(201);

  // No reload: the event stream invalidates the member's transaction list.
  await expect(member.getByText("Live coffee").first()).toBeVisible({ timeout: 10_000 });

  // The owner's webhook page, in the same session (logins are rate-limited).
  await owner.getByRole("link", { name: "Webhooks" }).first().click();
  await owner.getByLabel("Name").fill("Catcher");
  await owner.getByLabel("URL").fill("http://127.0.0.1:9/hook");
  await owner.getByRole("button", { name: "Add webhook" }).click();
  await expect(owner.getByTestId("webhook-secret")).toHaveText(/^whsec_[0-9a-f]{64}$/);
  await owner.getByRole("button", { name: "Send test" }).click();
  await expect(owner.getByText("webhook.test")).toBeVisible({ timeout: 10_000 });

  await ownerContext.close();
  await memberContext.close();
});
