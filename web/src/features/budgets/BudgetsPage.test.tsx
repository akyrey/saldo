// The budget screens against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const BUDGET = {
  id: "b1",
  name: "Groceries",
  active: true,
  sort_order: 0,
  notes: null,
  auto_budget: { type: "reset", period: "monthly", amount: "300.00", currency: "EUR" },
  carry: "leftover",
  created_at: "",
  updated_at: "t1",
};
const LIMIT = {
  id: "l1",
  budget_id: "b1",
  start_date: "2026-11-01",
  end_date: "2026-11-30",
  amount: "300.00",
  currency: "EUR",
  carried: "0.00",
  spent: "330.00",
  available: "-30.00",
  notes: null,
  created_at: "",
  updated_at: "",
};

function api(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/budgets": list([BUDGET]),
    "GET /api/v1/budgets/b1": { data: BUDGET },
    "GET /api/v1/budgets/b1/limits": list([LIMIT]),
    ...extra,
  });
}

describe("budgets", () => {
  it("shows each budget's progress in the chosen month and asks for that month", async () => {
    const seen = api();
    renderApp("/budgets?month=2026-11");

    const bar = await screen.findByRole("progressbar", { name: "Spent 330.00 of 300.00 EUR" });
    expect(bar).toHaveAttribute("aria-valuenow", "100");
    expect(screen.getByText(/Over by/)).toBeInTheDocument();
    const limits = seen.find((r) => r.url.includes("/limits"));
    expect(new URL(limits?.url ?? "").searchParams.get("from")).toBe("2026-11-01");
    expect(new URL(limits?.url ?? "").searchParams.get("to")).toBe("2026-11-30");

    await userEvent.setup().click(screen.getByRole("button", { name: "Next month" }));
    expect(await screen.findByText(/December 2026/)).toBeInTheDocument();
  });

  it("adds what the previous limit carried in to the bar", async () => {
    api({
      "GET /api/v1/budgets/b1/limits": list([{ ...LIMIT, carried: "100.00", available: "70.00" }]),
    });
    renderApp("/budgets?month=2026-11");

    const bar = await screen.findByRole("progressbar", { name: "Spent 330.00 of 400.00 EUR" });
    expect(bar).toHaveAttribute("aria-valuenow", "82");
    expect(screen.getByText(/carried/)).toBeInTheDocument();
    expect(screen.getByText(/Left:/)).toBeInTheDocument();
  });

  it("creates a budget with an automatic limit", async () => {
    const seen = api({ "POST /api/v1/budgets": { data: BUDGET } });
    renderApp("/budgets?month=2026-11");
    const user = userEvent.setup();
    const form = await screen.findByRole("form", { name: "New budget" });

    await user.type(within(form).getByLabelText("Name"), "Fun");
    await user.selectOptions(within(form).getByLabelText("Automatic limit"), "reset");
    await user.type(within(form).getByLabelText("Amount"), "50,5");
    await user.click(within(form).getByRole("button", { name: "Add budget" }));

    await waitFor(() => expect(seen.some((r) => r.method === "POST")).toBe(true));
    const post = seen.find((r) => r.method === "POST");
    expect(await post?.json()).toEqual({
      name: "Fun",
      active: true,
      notes: null,
      auto_budget: { type: "reset", period: "monthly", amount: "50.5" },
      carry: "none",
    });
  });

  it("edits limits on the budget page", async () => {
    const seen = api({
      "POST /api/v1/budgets/b1/limits": { data: LIMIT },
      "PATCH /api/v1/budget-limits/l1": { data: LIMIT },
      "DELETE /api/v1/budget-limits/l1": new Response(null, { status: 204 }),
    });
    renderApp("/budgets/b1");
    const user = userEvent.setup();
    await screen.findByTestId("limit-l1");

    const add = screen.getByRole("form", { name: "Add limit" });
    await user.type(within(add).getByLabelText("Limit"), "120");
    await user.click(within(add).getByRole("button", { name: "Add limit" }));
    await waitFor(() => expect(seen.some((r) => r.method === "POST")).toBe(true));
    const body = (await seen.find((r) => r.method === "POST")?.json()) as { amount: string };
    expect(body.amount).toBe("120");

    await user.click(screen.getByRole("button", { name: "Edit limit 2026-11-01" }));
    const edit = screen.getByRole("form", { name: "Save limit" });
    const amount = within(edit).getByLabelText("Limit");
    await user.clear(amount);
    await user.type(amount, "350");
    await user.click(within(edit).getByRole("button", { name: "Save limit" }));
    await waitFor(() => expect(seen.some((r) => r.method === "PATCH")).toBe(true));

    await user.click(await screen.findByRole("button", { name: "Delete limit 2026-11-01" }));
    await waitFor(() => expect(seen.some((r) => r.method === "DELETE")).toBe(true));
  });
});
