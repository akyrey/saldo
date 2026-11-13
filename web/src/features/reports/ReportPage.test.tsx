// The dashboard and report pages against a fake API.
import { screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, ME, MEMBERS } from "@/test/fake-api";
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
          },
        ],
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
        items: [{ id: "c1", name: "Food", income: "0.00", expenses: "59.75", intervals: [] }],
      },
    ],
  },
};

function api() {
  return fakeApi({
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
});
