import { expect, test } from "@playwright/test";
import { choose, createAccount, login } from "./helpers";

// Runs after flows.spec.ts (files run in name order), which did the setup.
// One login only: the suite shares the login rate limit (10 per 15 minutes).
test("create and fire a recurring transaction, and guard a piggy bank", async ({ page }) => {
  await login(page);
  await createAccount(page, "Rent checking", "asset");
  await createAccount(page, "Rent landlord", "expense");

  await page.goto("/recurrences/new");
  await page.getByLabel("Title").fill("Monthly rent");
  const repetition = page.getByRole("group", { name: "Repetition 1" });
  await repetition.getByLabel("Repeats").selectOption("daily");
  await expect(repetition.getByText("Every day")).toBeVisible();
  await expect(
    page.getByRole("region", { name: "Upcoming dates" }).getByRole("listitem").first(),
  ).toBeVisible();
  const split = page.getByRole("group", { name: "Split 1" });
  await split.getByLabel("Description").fill("Rent");
  await choose(split.getByLabel("Source account"), "Rent checking (EUR)");
  await choose(split.getByLabel("Destination account"), "Rent landlord (EUR)");
  await split.getByLabel(/^Amount/).fill("800");
  await page.getByRole("button", { name: "Create recurring transaction" }).click();

  const item = page.getByRole("listitem", { name: "Monthly rent" });
  await expect(item.getByText("Every day")).toBeVisible();
  await item.getByRole("button", { name: /create today/i }).click();
  await expect(item.getByRole("status")).toHaveText(
    /Created the transaction for \d{4}-\d{2}-\d{2}\.|Already created for/,
  );
  // Firing again for the same day creates nothing new.
  await item.getByRole("button", { name: /create today/i }).click();
  await expect(item.getByRole("status")).toHaveText(/Already created for/);

  await page.goto("/transactions?q=Rent");
  // One row: the second firing created nothing.
  await expect(page.getByRole("row", { name: /Rent checking to Rent landlord/ })).toHaveCount(1);

  // The account holds nothing beyond the rent paid, so saving money is refused.
  await page.goto("/piggy-banks");
  await page.getByRole("button", { name: /new piggy bank/i }).click();
  const form = page.getByRole("form", { name: "New piggy bank" });
  await form.getByLabel("Name").fill("Holiday");
  await form.getByLabel(/^Target \(/).fill("300");
  for (const box of await form.getByRole("checkbox").all()) await box.uncheck();
  await form.getByLabel("Rent checking").check();
  await form.getByRole("button", { name: "Create piggy bank" }).click();
  const card = page.getByRole("listitem", { name: "Holiday" });
  await card.getByRole("button", { name: /add money/i }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel(/Amount/).fill("10");
  await dialog.getByRole("button", { name: "Add" }).click();
  await expect(dialog.getByText(/exceeds the account balance/)).toBeVisible();
});
