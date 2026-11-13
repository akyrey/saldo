import { expect, type Page } from "@playwright/test";

export const OWNER = {
  name: "Ada Owner",
  email: "ada@example.com",
  password: "a long enough password",
  household: "Home",
};

export async function login(page: Page, email = OWNER.email, password = OWNER.password) {
  await page.goto("/login");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
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
