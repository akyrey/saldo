import { expect, type BrowserContext, type Locator, type Page } from "@playwright/test";

export const OWNER = {
  name: "Ada Owner",
  email: "ada@example.com",
  password: "a long enough password",
  household: "Home",
};

type Cookies = Awaited<ReturnType<BrowserContext["cookies"]>>;

/**
 * Sessions by email. The server allows 10 logins per 15 minutes from one IP,
 * so flows reuse a session instead of logging in each time (one worker, so a
 * module-level cache is shared by every flow).
 */
const sessions = new Map<string, Cookies>();

export async function login(page: Page, email = OWNER.email, password = OWNER.password) {
  const cookies = sessions.get(email);
  if (cookies) {
    await page.context().addCookies(cookies);
    // A flow may have logged this session out: then log in again.
    const me = await page.request.get("/api/v1/auth/me");
    if (me.ok()) {
      await page.goto("/accounts");
      await expect(page.getByRole("heading", { name: "Accounts", exact: true })).toBeVisible();
      return;
    }
    sessions.delete(email);
    await page.context().clearCookies();
  }
  await page.goto("/login");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
  // Logging in opens the home page (transactions)...
  await expect(page.getByRole("heading", { name: "Transactions", exact: true })).toBeVisible();
  sessions.set(email, await page.context().cookies());
  // ...but flows start on the accounts page, as with a reused session above.
  await page.goto("/accounts");
  await expect(page.getByRole("heading", { name: "Accounts", exact: true })).toBeVisible();
}

/** Creates an account through the API with the page's session cookie. */
export async function createAccount(page: Page, name: string, type: string): Promise<string> {
  const origin = new URL(page.url()).origin;
  const response = await page.request.post("/api/v1/accounts", {
    headers: { "X-Saldo-Csrf": "1", Origin: origin },
    data: { name, type },
  });
  expect(response.status(), await response.text()).toBe(201);
  const body = (await response.json()) as { data: { id: string } };
  return body.data.id;
}

/**
 * Picks an option in an autocomplete field: types `typed` (default: the
 * label), then clicks the option whose label is exactly `label`.
 */
export async function choose(field: Locator, label: string, typed = label): Promise<void> {
  await field.fill(typed);
  const listId = (await field.getAttribute("aria-controls")) ?? "";
  const listbox = field.page().locator(`[id="${listId}"]`);
  await listbox
    .getByRole("option")
    .filter({ has: field.page().locator("span").getByText(label, { exact: true }) })
    .first()
    .click();
  await expect(field).toHaveValue(label);
}
