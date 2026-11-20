import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { accountRoutes, CURRENCIES, fakeApi, json, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => vi.unstubAllGlobals());

const BIG = {
  id: "v1",
  name: "Big spends",
  page: "transactions",
  filters: { amount_min: "100", kind: "withdrawal" },
  sort_order: 0,
  created_at: "",
  updated_at: "",
};

function api(views: unknown[] = [BIG]) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    ...accountRoutes([], []),
    "GET /api/v1/categories": list([]),
    "GET /api/v1/tags": list([]),
    "GET /api/v1/budgets": list([]),
    "GET /api/v1/bills": list([]),
    "GET /api/v1/transactions": list([]),
    "GET /api/v1/saved-views": { data: views },
    "POST /api/v1/saved-views": async (request: Request) =>
      json(201, { data: { ...BIG, id: "v2", ...(await request.json()) } }),
  });
}

describe("SavedViewChips", () => {
  it("applies a saved view and the Uncategorised chip", async () => {
    api();
    const { router } = renderApp("/transactions?q=rent");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Big spends" }));
    await waitFor(() =>
      expect(router.state.location.search).toEqual({ amount_min: "100", kind: "withdrawal" }),
    );
    expect(screen.getByRole("button", { name: "Big spends" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await user.click(screen.getByRole("button", { name: "Uncategorised" }));
    await waitFor(() => expect(router.state.location.search).toEqual({ category_id: "none" }));
  });

  it("saves the current filters under a name", async () => {
    const seen = api([]);
    renderApp("/transactions?kind=deposit&reconciled=false");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /Save view/ }));
    await user.type(screen.getByLabelText("View name"), "Unconfirmed income");
    await user.click(screen.getByRole("button", { name: /^Save$/ }));
    await waitFor(() => expect(seen.some((r) => r.method === "POST")).toBe(true));
    const post = seen.find((r) => r.method === "POST");
    expect(await post?.json()).toEqual({
      name: "Unconfirmed income",
      page: "transactions",
      filters: { kind: "deposit", reconciled: false },
    });
  });
});
