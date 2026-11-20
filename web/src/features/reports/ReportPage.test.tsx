// The dashboard and report pages against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

// Recharts measures its container; jsdom has no layout, so give it a size.
beforeAll(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const RANGE = { from: "2026-11-01", to: "2026-11-30", interval: "month" };
const CASH_FLOW = {
  data: {
    range: RANGE,
    currencies: [
      {
        currency: "EUR",
        income: "2500.00",
        expenses: "59.75",
        net: "2440.25",
        intervals: [
          {
            start: "2026-11-01",
            end: "2026-11-30",
            income: "2500.00",
            expenses: "59.75",
            net: "2440.25",
            savings_rate_bp: 9761,
          },
        ],
        savings_rate_bp: 9761,
      },
    ],
  },
};
const NET_WORTH = {
  data: {
    range: RANGE,
    currencies: [
      {
        currency: "EUR",
        opening: "100.00",
        intervals: [
          { start: "2026-11-01", end: "2026-11-30", value: "2540.25", change: "2440.25" },
        ],
      },
    ],
  },
};
const BY_CATEGORY = {
  data: {
    range: RANGE,
    currencies: [
      {
        currency: "EUR",
        items: [
          { id: "c1", name: "Food", income: "0.00", expenses: "59.75", intervals: [] },
          { id: "c2", name: "Rent", income: "0.00", expenses: "900.00", intervals: [] },
          { id: "c3", name: "Bars", income: "0.00", expenses: "8.50", intervals: [] },
        ],
      },
    ],
  },
};

const FORECAST = {
  data: {
    range: { from: "2026-11-17", to: "2027-02-17", interval: "week" },
    currencies: [
      {
        currency: "EUR",
        start_balance: "500.00",
        points: [
          { date: "2026-11-22", balance: "500.00" },
          { date: "2026-12-06", balance: "-300.00" },
        ],
        lowest: { date: "2026-12-01", balance: "-300.00" },
        items: [
          {
            date: "2026-12-01",
            source: "recurrence",
            source_id: "r1",
            title: "Rent",
            account_id: "checking",
            amount: "-800.00",
          },
          {
            date: "2026-12-01",
            source: "bill",
            source_id: "b1",
            title: "Insurance",
            account_id: null,
            amount: "0.00",
          },
        ],
      },
    ],
  },
};
const ACCOUNTS = list([
  { id: "checking", name: "Checking", type: "asset", currency: "EUR", active: true },
  { id: "shop", name: "Shop", type: "expense", currency: "EUR", active: true },
]);

function api() {
  return fakeApi({
    "GET /api/v1/reports/forecast": FORECAST,
    "GET /api/v1/accounts": ACCOUNTS,
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/reports/cash-flow": CASH_FLOW,
    "GET /api/v1/reports/net-worth": NET_WORTH,
    "GET /api/v1/reports/by-category": BY_CATEGORY,
  });
}

describe("reports", () => {
  it("shows net worth, cash flow and this month's categories on the dashboard", async () => {
    api();
    renderApp("/reports");
    expect(await screen.findByTestId("net-worth-now")).toHaveTextContent("2,540.25");
    expect(await screen.findByText("Food")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Income and expenses per month" })).toBeInTheDocument();
    expect(await screen.findByTestId("next-30-lowest")).toHaveTextContent("2026-12-01");
  });

  it("shows the forecast, warns below zero and changes the horizon and account", async () => {
    const seen = api();
    renderApp("/forecast");
    expect(await screen.findByTestId("forecast-lowest-EUR")).toHaveTextContent("-€300.00");
    expect(screen.getByRole("alert")).toHaveTextContent("below zero on 2026-12-01");
    expect(screen.getByText(/Recurring · Checking/)).toBeInTheDocument();
    expect(screen.getByText(/Bill · account unknown/)).toBeInTheDocument();
    const lastQuery = () =>
      new URL(seen.filter((r) => r.url.includes("/forecast")).at(-1)?.url ?? "").searchParams;
    expect(lastQuery().get("interval")).toBe("week");

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "1 yr" }));
    await waitFor(() => expect(lastQuery().get("interval")).toBe("month"));
    const account = screen.getByLabelText("Account");
    expect(within(account).queryByText("Shop")).toBeNull();
    await user.selectOptions(account, "checking");
    await waitFor(() => expect(lastQuery().get("account_id")).toBe("checking"));
  });

  it("passes span, interval and scope to the report and shows its table", async () => {
    const seen = api();
    renderApp("/reports/cash-flow?from=2026-11-01&to=2026-11-30&interval=week&scope=mine");
    expect(await screen.findByTestId("net-EUR")).toHaveTextContent("2,440.25");
    expect(screen.getByRole("table")).toHaveTextContent("59.75");
    await waitFor(() => expect(seen.some((r) => r.url.includes("cash-flow"))).toBe(true));
    const url = new URL(seen.find((r) => r.url.includes("cash-flow"))?.url ?? "");
    expect(Object.fromEntries(url.searchParams)).toMatchObject({
      from: "2026-11-01",
      to: "2026-11-30",
      interval: "week",
      scope: "mine",
    });
  });

  it("sorts the table by a column, then reverses it, then restores the server's order", async () => {
    api();
    const user = userEvent.setup();
    renderApp("/reports/by-category?from=2026-11-01&to=2026-11-30");
    const table = await screen.findByRole("table");
    const names = () =>
      within(table)
        .getAllByRole("row")
        .slice(1)
        .map((row) => within(row).getAllByRole("cell")[0]?.textContent);
    const expenses = within(table).getByRole("columnheader", { name: /Expenses/ });
    expect(names()).toEqual(["Food", "Rent", "Bars"]);
    expect(expenses).toHaveAttribute("aria-sort", "none");

    await user.click(within(expenses).getByRole("button"));
    expect(names()).toEqual(["Bars", "Food", "Rent"]);
    expect(expenses).toHaveAttribute("aria-sort", "ascending");

    await user.click(within(expenses).getByRole("button"));
    expect(names()).toEqual(["Rent", "Food", "Bars"]);
    expect(expenses).toHaveAttribute("aria-sort", "descending");

    await user.click(within(expenses).getByRole("button"));
    expect(names()).toEqual(["Food", "Rent", "Bars"]);
  });

  it("shows the savings rate as a percentage", async () => {
    api();
    renderApp("/reports/savings-rate?from=2026-11-01&to=2026-11-30");
    expect(await screen.findByTestId("savings-rate-EUR")).toHaveTextContent("97.61%");
  });

  it("compares years with their own controls and no date span", async () => {
    const seen = fakeApi({
      "GET /api/v1/auth/me": ME,
      "GET /api/v1/household/members": MEMBERS,
      "GET /api/v1/currencies": CURRENCIES,
      "GET /api/v1/categories": list([]),
      "GET /api/v1/reports/year-over-year": {
        data: {
          metric: "expenses",
          category_id: null,
          currencies: [
            {
              currency: "EUR",
              years: [
                {
                  year: 2025,
                  months: Array(12).fill("10.00"),
                  month_change_bp: Array(12).fill(null),
                  total: "120.00",
                  change: null,
                  change_bp: null,
                },
                {
                  year: 2026,
                  months: [...Array(11).fill("15.00"), null],
                  month_change_bp: [...Array(11).fill(5000), null],
                  total: "165.00",
                  change: "55.00",
                  change_bp: 5000,
                },
              ],
            },
          ],
        },
      },
    });
    renderApp("/reports/year-over-year?years=2&metric=expenses");
    expect(await screen.findByText(/against the same months of 2025/)).toHaveTextContent("50.00%");
    expect(screen.getByLabelText("Years")).toHaveValue("2");
    expect(screen.queryByLabelText("Interval")).not.toBeVisible();
    const request = seen.find((r) => r.url.includes("year-over-year"));
    expect(new URL(request?.url ?? "").searchParams.get("years")).toBe("2");
  });
});
