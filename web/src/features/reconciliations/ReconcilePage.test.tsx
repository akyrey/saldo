// Reconciling an account against a fake API.
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const DRAFT = {
  account_id: "a1",
  currency: "EUR",
  statement_date: "2026-11-10",
  reconciled_through: "2026-10-31",
  cleared_balance: "100.00",
  splits: [
    { id: "s1", group_id: "g1", date: "2026-11-01", description: "Salary", amount: "2000.00" },
    { id: "s2", group_id: "g2", date: "2026-11-03", description: "Shop", amount: "-50.00" },
  ],
};
const DONE = {
  id: "r2",
  account_id: "a1",
  statement_date: "2026-11-10",
  statement_balance: "2050.00",
  currency: "EUR",
  difference: "0.00",
  difference_group_id: null,
  split_ids: ["s1", "s2"],
  created_by_user_id: null,
  created_at: "",
};

function api() {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts/a1": { data: account("a1", "Checking", "asset") },
    "GET /api/v1/accounts/a1/balance": { data: { balance: "2050.00", currency: "EUR" } },
    "GET /api/v1/accounts": list([account("a1", "Checking", "asset")]),
    "GET /api/v1/categories": list([]),
    "GET /api/v1/transactions": list([]),
    "GET /api/v1/accounts/a1/reconciliations/draft": { data: DRAFT },
    "GET /api/v1/accounts/a1/reconciliations": list([DONE, { ...DONE, id: "r1" }]),
    "POST /api/v1/accounts/a1/reconciliations": { data: DONE },
    "DELETE /api/v1/reconciliations/r2": new Response(null, { status: 204 }),
  });
}

describe("reconciliation", () => {
  it("ticks transactions off until the difference is zero, then finishes", async () => {
    const seen = api();
    renderApp("/accounts/a1/reconcile?date=2026-11-10&balance=2050.00");
    const user = userEvent.setup();
    expect(await screen.findByText("Reconciled up to 2026-10-31.")).toBeInTheDocument();
    expect(screen.getByTestId("reconcile-difference")).toHaveTextContent("1,950.00");
    expect(screen.getByRole("button", { name: "Finish" })).toBeDisabled();

    await user.click(screen.getByLabelText("2026-11-01 Salary"));
    expect(screen.getByTestId("reconcile-difference")).toHaveTextContent("-€50.00");
    await user.click(screen.getByLabelText(/Select all 2/));
    expect(screen.getByTestId("reconcile-difference")).toHaveTextContent("0.00");
    await user.click(screen.getByRole("button", { name: "Finish" }));

    await waitFor(() => expect(seen.some((r) => r.method === "POST")).toBe(true));
    const body = await seen.find((r) => r.method === "POST")?.json();
    expect(body).toEqual({
      statement_date: "2026-11-10",
      // The router reads the URL's 2050.00 as a number.
      statement_balance: "2050",
      split_ids: ["s1", "s2"],
      book_difference: false,
    });
  });

  it("shows the history on the account page and undoes the latest", async () => {
    const seen = api();
    renderApp("/accounts/a1");
    expect(await screen.findByText("Reconciled up to 2026-11-10")).toBeInTheDocument();
    const undo = screen.getAllByRole("button", { name: "Undo" });
    expect(undo).toHaveLength(1);
    await userEvent.setup().click(undo[0] as HTMLElement);
    await waitFor(() => expect(seen.some((r) => r.method === "DELETE")).toBe(true));
    expect(screen.getByRole("link", { name: "Reconcile" })).toBeInTheDocument();
  });
});
