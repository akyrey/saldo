import { expect, type BrowserContext, type Page } from "@playwright/test";

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
  await expect(page.getByRole("heading", { name: "Accounts", exact: true })).toBeVisible();
  sessions.set(email, await page.context().cookies());
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
