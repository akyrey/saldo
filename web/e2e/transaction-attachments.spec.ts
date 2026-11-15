// Attachments on a transaction and on a bill, against the real server.
// Runs after flows.spec.ts, which sets up the household.
import { expect, test, type Page } from "@playwright/test";
import { createAccount, login } from "./helpers";

const RECEIPT = {
  name: "receipt.pdf",
  mimeType: "application/pdf",
  buffer: Buffer.from("%PDF-1.4 receipt"),
};

async function post(page: Page, path: string, data: object): Promise<string> {
  const origin = new URL(page.url()).origin;
  const response = await page.request.post(path, {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data,
  });
  expect(response.status(), await response.text()).toBe(201);
  return ((await response.json()) as { data: { id: string } }).data.id;
}

test("upload, download and delete attachments on a transaction and a bill", async ({ page }) => {
  await login(page);
  const purse = await createAccount(page, "Purse", "asset");
  const bakery = await createAccount(page, "Bakery", "expense");
  const groupId = await post(page, "/api/v1/transactions", {
    splits: [
      {
        date: "2026-11-01",
        description: "Bread",
        source_id: purse,
        destination_id: bakery,
        amount: "3.20",
      },
    ],
  });

  await page.goto(`/transactions/${groupId}/edit`);
  await expect(page.getByText("No files yet.")).toBeVisible();
  await page.getByLabel("File").setInputFiles(RECEIPT);
  await page.getByLabel("Title (optional)").fill("Bakery receipt");
  await page.getByRole("button", { name: "Upload" }).click();
  await expect(page.getByText("Bakery receipt")).toBeVisible();
  await expect(page.getByText("receipt.pdf · 16 B")).toBeVisible();

  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download Bakery receipt" }).click();
  const download = await downloading;
  expect(download.suggestedFilename()).toBe("receipt.pdf");

  await page.getByRole("button", { name: "Delete Bakery receipt" }).click();
  await page.getByRole("button", { name: "Really delete Bakery receipt?" }).click();
  await expect(page.getByText("No files yet.")).toBeVisible();

  const billId = await post(page, "/api/v1/bills", {
    name: "Bakery subscription",
    amount_min: "10.00",
    amount_max: "10.00",
    first_due_date: "2026-12-01",
    repeat_freq: "monthly",
  });
  await page.goto(`/bills/${billId}`);
  await page.getByLabel("File").setInputFiles(RECEIPT);
  await page.getByRole("button", { name: "Upload" }).click();
  await expect(page.getByText("receipt.pdf", { exact: true })).toBeVisible();
});
