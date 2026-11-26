// The credit card panel on an account page, against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, apiError, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const CARD = account("c1", "Visa", "asset", { asset_role: "credit_card" });
const CHECKING = account("a1", "Checking", "asset");
const DOLLARS = account("a2", "Dollars", "asset", { currency: "USD" });
const PLAN = {
  account_id: "c1",
  currency: "EUR",
  settlement_account_id: "a1",
  settlement_day: 15,
  starts_cycle: "2026-11",
  limits: [{ from_cycle: "2026-11", amount: "500.00" }],
  created_at: "",
  updated_at: "",
};
const NOVEMBER = {
  cycle: "2026-11",
  from: "2026-11-01",
  to: "2026-11-30",
  currency: "EUR",
  limit: "500.00",
  spent: "100.00",
  available: "400.00",
  moved_in: "0.00",
  moved_out: "50.00",
  settlement: { state: "synced", date: "2026-12-15", group_id: "g9", amount: "100.00" },
  difference_after_settlement: null,
};
const SPLITS = [
  {
    split_id: "s1",
    group_id: "g1",
    date: "2026-11-03",
    description: "Groceries",
    counterparty: { id: "e1", name: "Shop" },
    amount: "100.00",
    moved_in: false,
    moved_out: false,
  },
  {
    split_id: "s2",
    group_id: "g2",
    date: "2026-11-30",
    description: "Late dinner",
    counterparty: { id: "e2", name: "Bistro" },
    amount: "50.00",
    moved_in: false,
    moved_out: true,
  },
];

function api(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts/c1": { data: CARD },
    "GET /api/v1/accounts/c1/balance": { data: { balance: "-100.00", currency: "EUR" } },
    "GET /api/v1/accounts": list([CARD, CHECKING, DOLLARS]),
    "GET /api/v1/categories": list([]),
    "GET /api/v1/transactions": list([]),
    "GET /api/v1/accounts/c1/reconciliations": list([]),
    "GET /api/v1/accounts/c1/credit-card": { data: PLAN },
    "GET /api/v1/accounts/c1/credit-card/cycles": { data: [NOVEMBER] },
    "GET /api/v1/accounts/c1/credit-card/cycles/2026-11": { data: { ...NOVEMBER, splits: SPLITS } },
    "GET /api/v1/accounts/c1/credit-card/cycles/2026-12": {
      data: { ...NOVEMBER, cycle: "2026-12", spent: "50.00", available: "450.00", splits: [] },
    },
    ...extra,
  });
}

describe("credit card panel", () => {
  it("shows what is left, the payment and the month's transactions", async () => {
    api();
    renderApp("/accounts/c1");
    const panel = await screen.findByTestId("credit-card-panel");
    expect(await within(panel).findByTestId("card-available")).toHaveTextContent("400.00");
    expect(
      within(panel).getByRole("progressbar", { name: "Spent 100.00 of 500.00 EUR" }),
    ).toHaveAttribute("aria-valuenow", "20");
    expect(within(panel).getByTestId("card-settlement")).toHaveTextContent(
      "Payment: €100.00 from Checking on 2026-12-15 updates until then",
    );
    const [first, second] = within(panel).getAllByRole("listitem");
    if (!first || !second) throw new Error("two rows expected");
    expect(first).toHaveTextContent("Groceries");
    expect(within(first).getByRole("button", { name: "Count in next month" })).toBeEnabled();
    expect(second).toHaveTextContent("to Dec");
    expect(within(second).getByRole("button", { name: "Move back" })).toBeEnabled();
  });

  it("moves a transaction and shows a refusal", async () => {
    const seen = api({
      "PUT /api/v1/accounts/c1/credit-card/moves/s1": new Response(null, { status: 204 }),
      "DELETE /api/v1/accounts/c1/credit-card/moves/s2": apiError(
        409,
        "conflict",
        "2026-11 is closed",
      ),
    });
    renderApp("/accounts/c1");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Count in next month" }));
    await waitFor(() =>
      expect(seen.some((r) => r.method === "PUT" && r.url.endsWith("/moves/s1"))).toBe(true),
    );
    await user.click(screen.getByRole("button", { name: "Move back" }));
    expect(await screen.findByText("2026-11 is closed")).toBeInTheDocument();
  });

  it("switches to the next month", async () => {
    api();
    renderApp("/accounts/c1");
    const user = userEvent.setup();
    await user.selectOptions(await screen.findByRole("combobox", { name: "Month" }), "2026-12");
    expect(await screen.findByText("No card transactions in this month.")).toBeInTheDocument();
    expect(screen.getByTestId("card-available")).toHaveTextContent("450.00");
  });

  it("sets up a plan, offering only accounts in the card's currency", async () => {
    const seen = api({
      "GET /api/v1/accounts/c1/credit-card": apiError(404, "not_found"),
      "PUT /api/v1/accounts/c1/credit-card": json201(),
    });
    renderApp("/accounts/c1");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Set up a monthly plan" }));
    const payer = screen.getByLabelText("Paid from");
    const options = within(payer)
      .getAllByRole("option")
      .map((o) => o.textContent);
    expect(options).toEqual(["Choose an account", "Checking"]);
    await user.type(screen.getByLabelText("Monthly budget (EUR)"), "1 500,00");
    await user.selectOptions(payer, "a1");
    await user.clear(screen.getByLabelText("Payment day of the next month"));
    await user.type(screen.getByLabelText("Payment day of the next month"), "31");
    await user.click(screen.getByRole("button", { name: "Set up plan" }));
    await waitFor(() => expect(seen.some((r) => r.method === "PUT")).toBe(true));
    const put = seen.find((r) => r.method === "PUT");
    expect(await put?.json()).toEqual({
      monthly_limit: "1500.00",
      settlement_account_id: "a1",
      settlement_day: 31,
    });
  });

  it("changes the budget from a chosen month", async () => {
    const seen = api({
      "PUT /api/v1/accounts/c1/credit-card/limits/2026-12": { data: PLAN },
    });
    renderApp("/accounts/c1");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Change budget" }));
    await user.type(screen.getByLabelText("New budget (EUR)"), "600");
    await user.selectOptions(screen.getByLabelText("From"), "2026-12");
    await user.click(screen.getByRole("button", { name: "Change budget" }));
    await waitFor(() =>
      expect(seen.some((r) => r.method === "PUT" && r.url.endsWith("/limits/2026-12"))).toBe(true),
    );
  });
});

function json201() {
  return new Response(JSON.stringify({ data: PLAN }), {
    status: 201,
    headers: { "Content-Type": "application/json" },
  });
}

describe("accounts list", () => {
  it("shows what is left on a card under its balance", async () => {
    const balance = (id: string) => ({ account_id: id, balance: "-100.00", currency: "EUR" });
    api({
      "GET /api/v1/accounts/balances": list([balance("c1"), balance("a1")]),
      "GET /api/v1/credit-cards": {
        data: [{ account_id: "c1", account_name: "Visa", current: NOVEMBER }],
      },
    });
    renderApp("/accounts");
    // The fake API answers every type's section with the same accounts.
    const [left] = await screen.findAllByTestId("card-left-Visa");
    expect(left).toHaveTextContent("€400.00 left of €500.00");
    expect(screen.queryByTestId("card-left-Checking")).not.toBeInTheDocument();
  });
});
