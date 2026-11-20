// The bill pages inside the whole app, against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, list, ME, MEMBERS, split } from "@/test/fake-api";
import { choose } from "@/test/combobox";
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

/** The periods table (the only one with a Status column), once its rows have loaded. */
async function periodsCard(): Promise<HTMLElement> {
  const header = await screen.findByRole("columnheader", { name: "Status" });
  const table = header.closest<HTMLElement>("table");
  if (!table) throw new Error("no periods table");
  await within(table).findByText("2026-11-02");
  return table;
}

const GYM = { ...BILL, id: "b2", name: "Gym", active: false, next_due_date: null };

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

function payment(n: number) {
  const day = String((n % 28) + 1).padStart(2, "0");
  return {
    split_id: `ls${n}`,
    group_id: `lg${n}`,
    date: `2025-01-${day}`,
    description: `Rent payment ${n}`,
    amount: "800.00",
    currency: "EUR",
    source: { id: "a1", name: "Checking" },
    destination: { id: "a2", name: "Landlord" },
    due_date: null,
  };
}

const HISTORY = {
  bill_id: "b1",
  currency: "EUR",
  range: { from: "2026-10-02", to: "2026-12-02" },
  amount_min: "800.00",
  amount_max: "800.00",
  due_dates: [
    { date: "2026-10-02", status: "paid", period_id: "p1", paid_split_id: "s0" },
    { date: "2026-11-02", status: "open", period_id: "p2", paid_split_id: null },
  ],
  payments: [
    {
      ...payment(0),
      split_id: "s0",
      group_id: "g0",
      date: "2026-10-03",
      description: "October rent",
      due_date: "2026-10-02",
    },
    { ...payment(1), currency: "USD", amount: "5.00", description: "Wire fee" },
  ],
  summary: {
    count: 1,
    total: "800.00",
    average: "800.00",
    min: "800.00",
    max: "800.00",
    other_currency_count: 1,
  },
  truncated: false,
};

function api(history: unknown = HISTORY) {
  return fakeApi({
    "GET /api/v1/bills/b1/history": { data: history },
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([
      account("a1", "Checking", "asset"),
      account("a2", "Landlord", "expense"),
    ]),
    "GET /api/v1/bills": list([GYM, BILL]),
    "PATCH /api/v1/bills/b1": { data: { ...BILL, active: false } },
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
    "GET /api/v1/attachments": { data: [] },
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

  it("folds inactive bills into a closed section below the active ones", async () => {
    api();
    renderApp("/bills");
    await screen.findByRole("link", { name: "Rent" });
    const gym = screen.getByRole("link", { name: "Gym" });
    const section = gym.closest("details");
    expect(section).not.toBeNull();
    expect(section?.open).toBe(false);
    expect(within(section as HTMLElement).getByText("Inactive (1)")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Rent" }).closest("details")).toBeNull();
  });

  it("turns a bill off from the list", async () => {
    const seen = api();
    renderApp("/bills");
    const toggle = await screen.findByRole("switch", { name: "Rent active" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await userEvent.click(toggle);
    await waitFor(() => expect(seen.some((r) => r.method === "PATCH")).toBe(true));
    const patch = seen.find((r) => r.method === "PATCH");
    expect(new URL(patch?.url ?? "http://x").pathname).toBe("/api/v1/bills/b1");
    expect(await patch?.json()).toEqual({ active: false });
  });

  it("skips a period and marks one paid by picking a transaction", async () => {
    const seen = api();
    renderApp("/bills/b1");
    const open = rowOf(within(await periodsCard()).getByText("2026-11-02"), "tr");
    await userEvent.click(within(open).getByRole("button", { name: "Skip" }));
    await waitFor(() =>
      expect(seen.some((r) => new URL(r.url).pathname === "/api/v1/bill-periods/p2/skip")).toBe(
        true,
      ),
    );

    await userEvent.click(within(open).getByRole("button", { name: "Mark paid" }));
    const picker = await screen.findByLabelText("Paying transaction");
    await choose(picker, "2026-11-07 · split s9", "s9");
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
    const paid = rowOf(within(await periodsCard()).getByText("2026-10-02"), "tr");
    expect(within(paid).getByText("paid")).toBeInTheDocument();
    expect(within(paid).getByRole("button", { name: "Reopen" })).toBeInTheDocument();
  });

  it("offers the bill's attachments", async () => {
    const seen = api();
    renderApp("/bills/b1");
    expect(await screen.findByText("Attachments")).toBeInTheDocument();
    await screen.findByText("No files yet.");
    const query = seen.find((r) => new URL(r.url).pathname === "/api/v1/attachments");
    const params = new URL(query?.url ?? "http://x").searchParams;
    expect([params.get("attachable_type"), params.get("attachable_id")]).toEqual(["bill", "b1"]);
  });

  it("summarises and lists the linked transactions", async () => {
    api();
    renderApp("/bills/b1");
    const summary = (await screen.findByText("Total paid")).closest("dl") as HTMLElement;
    expect(within(summary).getAllByText(/800\.00/)).toHaveLength(4);
    expect(
      screen.getByText(/1 payment\(s\) in another currency are listed below/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: /Payments from 2026-10-02 to 2026-12-02: 1 in EUR/ }),
    ).toBeInTheDocument();
    const october = rowOf(screen.getByRole("link", { name: "October rent" }), "tr");
    expect(screen.getByRole("link", { name: "October rent" })).toHaveAttribute(
      "href",
      "/transactions/g0/edit",
    );
    expect(within(october).getByText("Checking → Landlord")).toBeInTheDocument();
    expect(within(october).getByText("2026-10-02")).toBeInTheDocument();
    const fee = rowOf(screen.getByRole("link", { name: "Wire fee" }), "tr");
    expect(within(fee).getByText("Not matched to a due date")).toBeInTheDocument();
    expect(within(fee).getByText(/5\.00/)).toBeInTheDocument();
  });

  it("shows 25 linked transactions, then more on request", async () => {
    const payments = Array.from({ length: 30 }, (_, n) => payment(n));
    api({ ...HISTORY, payments });
    renderApp("/bills/b1");
    await screen.findByRole("link", { name: "Rent payment 0" });
    expect(screen.queryByRole("link", { name: "Rent payment 25" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Show more (5 left)" }));
    expect(screen.getByRole("link", { name: "Rent payment 29" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Show more/ })).toBeNull();
  });

  it("says when nothing is linked yet", async () => {
    api({
      ...HISTORY,
      payments: [],
      summary: {
        ...HISTORY.summary,
        count: 0,
        total: null,
        average: null,
        min: null,
        max: null,
        other_currency_count: 0,
      },
    });
    renderApp("/bills/b1");
    expect(await screen.findAllByText("No transactions linked to this bill yet.")).toHaveLength(2);
    expect(screen.getByText("Payment history")).toBeInTheDocument();
  });
});
