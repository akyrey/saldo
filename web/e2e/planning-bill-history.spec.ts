// Phase 11: a bill's linked transactions, payment chart and summary, against
// the real server. Runs after flows.spec.ts, which sets up the household.
import { expect, test, type Page } from "@playwright/test";
import { choose, createAccount, login } from "./helpers";

async function post(page: Page, path: string, data: object): Promise<string> {
  const origin = new URL(page.url()).origin;
  const response = await page.request.post(path, {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data,
  });
  expect(response.status(), await response.text()).toBe(201);
  return ((await response.json()) as { data: { id: string } }).data.id;
}

/** The ISO date of `day` in the month `offset` months from now. */
function dayOfMonth(offset: number, day: number): string {
  const now = new Date();
  const date = new Date(Date.UTC(now.getFullYear(), now.getMonth() + offset, day));
  return date.toISOString().slice(0, 10);
}

test("payments linked by bill and by Mark paid show in the bill's history", async ({ page }) => {
  await login(page);
  const bank = await createAccount(page, "History bank", "asset");
  const club = await createAccount(page, "History club", "expense");
  const [firstDue, secondDue] = [dayOfMonth(-2, 10), dayOfMonth(-1, 10)];
  const billId = await post(page, "/api/v1/bills", {
    name: "Club membership",
    amount_min: "39.00",
    amount_max: "45.00",
    first_due_date: firstDue,
    repeat_freq: "monthly",
  });
  const payment = (date: string, amount: string, description: string, bill?: string) =>
    post(page, "/api/v1/transactions", {
      splits: [
        {
          date,
          description,
          source_id: bank,
          destination_id: club,
          amount,
          ...(bill ? { bill_id: bill } : {}),
        },
      ],
    });
  // Linked by its bill: it pays the first due date by itself.
  await payment(dayOfMonth(-2, 11), "42.50", "Club fee one", billId);
  // Not linked yet: paid by hand below.
  const secondDate = dayOfMonth(-1, 12);
  await payment(secondDate, "45.00", "Club fee two");

  await page.goto(`/bills/${billId}`);
  const periods = page
    .getByRole("table")
    .filter({ has: page.getByRole("columnheader", { name: "Status" }) });
  const open = periods.getByRole("row").filter({ hasText: secondDue });
  await open.getByRole("button", { name: "Mark paid" }).click();
  await choose(
    page.getByLabel("Paying transaction"),
    `${secondDate} · Club fee two`,
    "Club fee two",
  );
  await page.getByRole("button", { name: "Confirm payment" }).click();

  const linked = page
    .getByRole("table")
    .filter({ has: page.getByRole("columnheader", { name: "Due date paid" }) });
  await expect(linked.getByRole("row").filter({ hasText: "Club fee two" })).toContainText(
    secondDue,
  );
  await expect(linked.getByRole("row").filter({ hasText: "Club fee one" })).toContainText(firstDue);
  await expect(page.getByRole("img", { name: /: 2 in EUR/ })).toBeVisible();
  await expect(page.locator("dl").getByText(/87\.50/)).toBeVisible();
});
