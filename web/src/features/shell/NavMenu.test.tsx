// The main navigation: four pages always visible, the rest behind "More".
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

function api(me: unknown) {
  return fakeApi({
    "GET /api/v1/auth/me": me,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/accounts/balances": list([]),
    "GET /api/v1/categories": list([]),
  });
}

/** The desktop bar (jsdom applies no CSS, so both bars are present). */
async function desktopNav() {
  return screen.findByRole("navigation", { name: "Main" });
}

describe("main navigation", () => {
  it("shows the main pages and keeps the others behind More", async () => {
    api(ME);
    renderApp("/accounts");
    const nav = await desktopNav();
    const visible = within(nav)
      .getAllByRole("link")
      .map((l) => l.textContent);
    expect(visible).toEqual(["Transactions", "Accounts", "Budgets", "Reports"]);
    expect(within(nav).queryByRole("link", { name: "Categories" })).toBeNull();

    const more = within(nav).getByRole("button", { name: "More" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(more);
    expect(more).toHaveAttribute("aria-expanded", "true");
    const menu = within(nav).getByRole("list", { name: "More pages" });
    expect(within(menu).getByRole("link", { name: "Export" })).toBeInTheDocument();
    expect(within(menu).getByRole("link", { name: "Webhooks" })).toBeInTheDocument();
  });

  it("closes the menu after choosing a page or pressing Escape", async () => {
    const seen = api(ME);
    const { router } = renderApp("/accounts");
    const nav = await desktopNav();
    const more = within(nav).getByRole("button", { name: "More" });

    await userEvent.click(more);
    await userEvent.keyboard("{Escape}");
    expect(more).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(more);
    await userEvent.click(within(nav).getByRole("link", { name: "Categories" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/categories"));
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(seen.length).toBeGreaterThan(0);
  });

  it("hides owner pages from members", async () => {
    api({ data: { ...ME.data, role: "member" } });
    renderApp("/accounts");
    const nav = await desktopNav();
    await userEvent.click(within(nav).getByRole("button", { name: "More" }));
    const menu = within(nav).getByRole("list", { name: "More pages" });
    expect(within(menu).getByRole("link", { name: "Rules" })).toBeInTheDocument();
    expect(within(menu).queryByRole("link", { name: "Export" })).toBeNull();
  });
});
