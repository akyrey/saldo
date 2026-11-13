// The bill pages inside the whole app, against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, list, ME, MEMBERS, split } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

/** The list item or table row around `element`. */
function rowOf(element: HTMLElement, tag: "li" | "tr"): HTMLElement {
  const row = element.closest<HTMLElement>(tag);
  if (!row) throw new Error(`no <${tag}> around the element`);
  return row;
}

const BILL = {
  id: "b1",
  name: "Rent",
  amount_min: "800.00",
  amount_max: "800.00",
  currency: "EUR",
  first_due_date: "2026-10-02",
  end_date: null,
  repeat_freq: "monthly",
  skip: 0,
  remind_days_before: 3,
  remind_every_days: 1,
  active: true,
  sort_order: 0,
  notes: null,
  next_due_date: "2026-11-02",
  created_at: "",
  updated_at: "",
};

function period(id: string, due: string, status: string) {
  return {
    id,
    bill_id: "b1",
    due_date: due,
    status,
    paid_split_id: status === "paid" ? "s0" : null,
    paid_at: null,
    last_reminded_on: null,
    reminder_count: 0,
    created_at: "",
    updated_at: "",
  };
}

function api() {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([
      account("a1", "Checking", "asset"),
      account("a2", "Landlord", "expense"),
    ]),
    "GET /api/v1/bills": list([BILL]),
    "GET /api/v1/bills/b1": { data: BILL },
    "GET /api/v1/bills/b1/periods": {
      data: [period("p2", "2026-11-02", "open"), period("p1", "2026-10-02", "paid")],
    },
    "GET /api/v1/transactions": list([
      {
        id: "g1",
        title: null,
        created_at: "",
        updated_at: "",
        splits: [split("s9", "a1", "a2", "800.00")],
      },
    ]),
    "POST /api/v1/bill-periods/p2/skip": { data: period("p2", "2026-11-02", "skipped") },
    "POST /api/v1/bill-periods/p2/pay": { data: period("p2", "2026-11-02", "paid") },
  });
}

describe("bill pages", () => {
  it("lists bills with their amount and next due date", async () => {
    api();
    renderApp("/bills");
    const row = rowOf(await screen.findByRole("link", { name: "Rent" }), "li");
    expect(within(row).getByText(/2026-11-02/)).toBeInTheDocument();
    expect(within(row).getByText(/800\.00/)).toBeInTheDocument();
  });

  it("skips a period and marks one paid by picking a transaction", async () => {
    const seen = api();
    renderApp("/bills/b1");
    const open = rowOf(await screen.findByText("2026-11-02"), "tr");
    await userEvent.click(within(open).getByRole("button", { name: "Skip" }));
    await waitFor(() =>
      expect(seen.some((r) => new URL(r.url).pathname === "/api/v1/bill-periods/p2/skip")).toBe(
        true,
      ),
    );

    await userEvent.click(within(open).getByRole("button", { name: "Mark paid" }));
    const picker = await screen.findByLabelText("Paying transaction");
    await userEvent.selectOptions(picker, "s9");
    await userEvent.click(screen.getByRole("button", { name: "Confirm payment" }));
    await waitFor(() => {
      const pay = seen.find((r) => new URL(r.url).pathname === "/api/v1/bill-periods/p2/pay");
      expect(pay).toBeDefined();
    });
    const pay = seen.find((r) => new URL(r.url).pathname === "/api/v1/bill-periods/p2/pay");
    expect(await pay?.json()).toEqual({ split_id: "s9" });
    // The picker looked around the due date.
    const search = seen.find((r) => new URL(r.url).pathname === "/api/v1/transactions");
    expect(new URL(search?.url ?? "http://x").searchParams.get("from")).toBe("2026-10-02");
  });

  it("shows the paid period as paid", async () => {
    api();
    renderApp("/bills/b1");
    const paid = rowOf(await screen.findByText("2026-10-02"), "tr");
    expect(within(paid).getByText("paid")).toBeInTheDocument();
    expect(within(paid).getByRole("button", { name: "Reopen" })).toBeInTheDocument();
  });
});
