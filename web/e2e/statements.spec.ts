// Phase 13 against the real server: a CSV statement imported with a new
// profile, committed, then reconciled from its closing balance. Runs after
// flows.spec.ts, which sets up the household.
import { expect, test } from "@playwright/test";
import { createAccount, login } from "./helpers";

const MAPPING = {
  date_column: ["Data"],
  date_formats: ["%d/%m/%Y"],
  amount: { kind: "signed", column: ["Importo"] },
  decimal_separator: ",",
  description_columns: [["Descrizione"]],
  opening_balance_label: "Saldo iniziale",
  closing_balance_label: "Saldo finale",
};

const CSV = [
  "Estratto conto",
  "Saldo iniziale;0,00",
  "Data;Descrizione;Importo",
  "05/11/2026;Coffee;-1,50",
  "06/11/2026;Refund from shop;20,00",
  "Saldo finale;18,50",
  "",
].join("\n");

test("a CSV statement is imported with a new profile, committed and reconciled", async ({
  page,
}) => {
  await login(page);
  const accountId = await createAccount(page, "Statement bank", "asset");

  // A profile for this bank, written as JSON and tried on the file first.
  await page.goto("/statement-profiles");
  await page.getByLabel("Name").fill("Statement bank CSV");
  await page.getByLabel("Account").selectOption({ label: "Statement bank" });
  await page.getByLabel("Format").selectOption("csv");
  await page.getByLabel("Mapping (JSON)").fill(JSON.stringify(MAPPING, null, 2));
  const file = { name: "november.csv", mimeType: "text/csv", buffer: Buffer.from(CSV) };
  await page.getByLabel("Sample file").setInputFiles(file);
  await page.getByRole("button", { name: "Preview" }).click();
  await expect(page.getByText(/2 lines, opening 0.00, closing 18.50/)).toBeVisible();
  await page.getByRole("button", { name: "Save profile" }).click();
  await expect(page.getByRole("listitem").filter({ hasText: "Statement bank CSV" })).toBeVisible();

  // Import from the account page; the review lists both lines as new.
  await page.goto(`/accounts/${accountId}`);
  await page.getByRole("link", { name: "Import statement" }).click();
  await page.getByLabel("Statement file").setInputFiles(file);
  await page.getByRole("button", { name: "Upload" }).click();
  await expect(page.getByText("Refund from shop", { exact: true })).toBeVisible();
  await expect(page.getByText("Row 6:")).toBeVisible();
  await page.getByRole("button", { name: /^Commit: 2 to create/ }).click();

  // Reconcile from the closing balance: both new transactions add up to it.
  await page.getByRole("link", { name: "Reconcile now" }).click();
  await expect(page.getByLabel("Statement balance")).toHaveValue("18.50");
  await page.getByLabel(/Select all 2/).check();
  await page.getByRole("button", { name: "Finish", exact: true }).click();
  await expect(page.getByText(/Reconciled up to 2026-11-06/)).toBeVisible();
  await expect(page.getByRole("region", { name: "Statement imports" })).toContainText("committed");
});

const ORDER = "405-7654321-7654321";

test("an Amazon export names a bank charge, which is split by item", async ({ page }) => {
  await login(page);
  const accountId = await createAccount(page, "Amazon bank", "asset");

  await page.goto("/sources");
  await page.getByLabel("Export").selectOption("amazon_orders");
  const orders = [
    "Order ID,Order Date,Currency,Total Amount,Product Name,Order Status,Payment Method Type,ASIN",
    `${ORDER},2026-11-02T10:00:00Z,EUR,15.00,Moka pot,Closed,Visa - 1,B1`,
    `${ORDER},2026-11-02T10:00:00Z,EUR,5.00,Filter,Closed,Visa - 1,B2`,
    "",
  ].join("\n");
  await page.getByLabel("File").setInputFiles({
    name: "orders.csv",
    mimeType: "text/csv",
    buffer: Buffer.from(orders),
  });
  await page.getByRole("button", { name: "Upload" }).click();
  await expect(page.getByRole("status")).toHaveText("2 new rows, 0 updated.");

  // The profile from the first flow reads this CSV too.
  await page.goto(`/accounts/${accountId}/import`);
  await page.getByLabel("Profile").selectOption({ label: "Statement bank CSV" });
  const statement = ["Data;Descrizione;Importo", `20/11/2026;AMAZON EU SARL ${ORDER};-19,00`, ""];
  await page.getByLabel("Statement file").setInputFiles({
    name: "amazon.csv",
    mimeType: "text/csv",
    buffer: Buffer.from(statement.join("\n")),
  });
  await page.getByRole("button", { name: "Upload" }).click();
  await expect(page.getByText("Amazon: Moka pot + 1 more")).toBeVisible();
  // The box shows the saved state once the server answers.
  await page.getByLabel(/^Split by item for/).click();
  await expect(page.getByLabel(/^Split by item for/)).toBeChecked();
  await page.getByRole("button", { name: /^Commit: 1 to create/ }).click();
  await expect(page.getByRole("button", { name: "Revert import" })).toBeVisible();

  const origin = new URL(page.url()).origin;
  const response = await page.request.get(`/api/v1/transactions?account_id=${accountId}`, {
    headers: { Origin: origin },
  });
  const body = (await response.json()) as {
    data: { title: string; splits: { description: string; amount: string }[] }[];
  };
  expect(body.data).toHaveLength(1);
  expect(body.data[0]?.title).toBe("Amazon: Moka pot + 1 more");
  expect(body.data[0]?.splits.map((s) => [s.description, s.amount])).toEqual([
    ["Moka pot", "14.00"],
    ["Filter", "5.00"],
  ]);
});
