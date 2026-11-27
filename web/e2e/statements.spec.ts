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
