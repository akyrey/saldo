import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, json, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";
import { progressWidth } from "./progress";

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
afterEach(() => vi.unstubAllGlobals());

const GOAL = {
  id: "g1",
  name: "Emergency fund",
  owner_user_id: null,
  target_amount: "3000.00",
  start_amount: "1000.00",
  currency: "EUR",
  target_date: "2027-02-28",
  reached_on: null,
  active: true,
  notes: null,
  progress: {
    current: "2500.00",
    remaining: "500.00",
    progress_bp: 7500,
    required_monthly: "125.00",
    average_monthly_change: "200.00",
    projected_date: "2027-01-13",
    on_track: true,
    reached: false,
  },
  created_at: "",
  updated_at: "",
};
const ZERO = "0.00";
const SAFE = {
  from: "2026-11-01",
  to: "2026-11-30",
  currency: "EUR",
  safe_to_spend: "-20.00",
  per_day: "-1.12",
  days_left: 18,
  parts: {
    income_received: "2000.00",
    income_planned: ZERO,
    spent: "1900.00",
    bills: "120.00",
    planned: ZERO,
    budgets: ZERO,
    piggy_banks: ZERO,
    savings_transfers: ZERO,
  },
  items: [
    {
      part: "bills",
      source: "bill",
      source_id: "b1",
      title: "Internet",
      date: "2026-11-20",
      amount: "120.00",
    },
  ],
};
const EMPTY_REPORT = { data: { range: {}, currencies: [] } };

function api(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/net-worth-goals": { data: [GOAL] },
    "GET /api/v1/reports/safe-to-spend": { data: SAFE },
    "GET /api/v1/reports/net-worth": EMPTY_REPORT,
    "GET /api/v1/reports/cash-flow": EMPTY_REPORT,
    "GET /api/v1/reports/by-category": EMPTY_REPORT,
    "GET /api/v1/reports/category-averages": {
      data: { months: 12, from: "", to: "", currencies: [] },
    },
    "GET /api/v1/reports/forecast": EMPTY_REPORT,
    "GET /api/v1/subscription-candidates": { data: [] },
    ...extra,
  });
}

describe("progressWidth", () => {
  it("turns basis points into a clamped percentage", () => {
    expect(progressWidth(7500)).toBe("75%");
    expect(progressWidth(-5)).toBe("0%");
    expect(progressWidth(12000)).toBe("100%");
  });
});

describe("dashboard goals and safe to spend", () => {
  it("shows a negative figure in red, its breakdown and the goals", async () => {
    api();
    renderApp("/reports");
    const figure = await screen.findByTestId("safe-to-spend");
    expect(figure).toHaveTextContent("-€20.00");
    expect(figure.className).toMatch(/text-destructive/);
    expect(screen.getByText(/a day for 18 days/)).toBeInTheDocument();
    expect(screen.getByText("Internet (2026-11-20)")).toBeInTheDocument();
    const goal = await screen.findByTestId("goal");
    expect(within(goal).getByText("On track")).toBeInTheDocument();
    expect(within(goal).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "75");
  });

  it("adds a goal from the net worth report", async () => {
    const seen = api({
      "POST /api/v1/net-worth-goals": () => json(201, { data: GOAL }),
    });
    renderApp("/reports/net-worth");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Name"), "House");
    await user.type(screen.getByLabelText("Target"), "50000,5");
    await user.click(screen.getByRole("button", { name: "Add goal" }));
    await waitFor(() => expect(seen.some((r) => r.method === "POST")).toBe(true));
    const post = seen.find((r) => r.method === "POST");
    expect(await post?.json()).toEqual({
      name: "House",
      target_amount: "50000.5",
      target_date: null,
    });
  });
});
