// Importing a statement against a fake API: upload, review, commit, revert.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, json, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const PROFILE = {
  id: "p1",
  name: "Widiba",
  account_id: "a1",
  format: "xlsx",
  mapping: {},
  part_accounts: {},
  transfer_rules: [],
  created_at: "",
  updated_at: "",
};

function line(position: number, status: string, action: string, extra: object = {}) {
  return {
    position,
    row: position + 2,
    accounting_date: "2026-11-05",
    value_date: "2026-11-05",
    booked_date: "2026-11-05",
    amount: "-1.50",
    description: `Line ${position}`,
    category: null,
    counterparty_name: null,
    counterparty_iban: null,
    external_id: null,
    ref_key: `k${position}`,
    notes: [],
    parts: [],
    status,
    action,
    reason: null,
    split_id: null,
    created_group_id: null,
    counterparty_account_id: null,
    category_id: null,
    proposal: null,
    use_proposal: true,
    split_by_item: false,
    ...extra,
  };
}

function detail(status: string, extra: object = {}) {
  return {
    import: {
      id: "i1",
      account_id: "a1",
      profile_id: "p1",
      filename: "november.xlsx",
      status,
      currency: "EUR",
      opening_balance: "100.00",
      closing_balance: "120.50",
      closing_date: "2026-11-30",
      gap: "0.00",
      summary: {
        counts: { new: 1, possible_match: 1 },
        unsupported: [{ row: 21, reason: "not a transaction" }],
        unsupported_total: 1,
        findings: [
          {
            kind: "not_on_statement",
            split_ids: ["s9"],
            date: "2026-11-12",
            amount: "-33.00",
            description: "Typed by hand",
          },
        ],
      },
      created_by_user_id: null,
      created_at: "",
      committed_at: status === "committed" ? "2026-11-30T10:00:00Z" : null,
      ...extra,
    },
    lines: [
      line(0, "possible_match", "link", { split_id: "s1", reason: "same amount, 1 day apart" }),
      line(1, "new", "create", { counterparty_name: "Bar Sport" }),
      line(2, "new", "create", {
        description: "AMAZON EU 405-1",
        proposal: {
          source: "amazon",
          description: "Amazon: Moka pot + 1 more",
          counterparty: "Amazon",
          notes: ["Amazon order 405-1 (2026-11-02): Moka pot; Filter"],
          items: [
            { product: "Moka pot", amount_minor: 1500 },
            { product: "Filter", amount_minor: 500 },
          ],
        },
      }),
    ],
  };
}

function api(committedAfterCommit = true) {
  let committed = false;
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts/a1": { data: account("a1", "Checking", "asset") },
    "GET /api/v1/accounts/a1/balance": { data: { balance: "0.00", currency: "EUR" } },
    "GET /api/v1/accounts": list([
      account("a1", "Checking", "asset"),
      account("x1", "Groceries", "expense"),
    ]),
    "GET /api/v1/categories": list([]),
    "GET /api/v1/transactions": list([]),
    "GET /api/v1/accounts/a1/reconciliations": list([]),
    "GET /api/v1/statement-profiles": { data: [PROFILE] },
    "GET /api/v1/statement-imports": list([detail("committed").import]),
    "POST /api/v1/statement-imports": { data: detail("draft") },
    "GET /api/v1/statement-imports/i1": () => ({
      data: detail(committed ? "committed" : "draft"),
    }),
    "PATCH /api/v1/statement-imports/i1/lines/1": async (request: Request) => ({
      data: line(1, "new", ((await request.json()) as { action?: string }).action ?? "create"),
    }),
    "PATCH /api/v1/statement-imports/i1/lines/2": async (request: Request) => ({
      data: { ...detail("draft").lines[2], ...((await request.json()) as object) },
    }),
    "POST /api/v1/statement-imports/i1/commit": () => {
      committed = committedAfterCommit;
      return { data: detail("committed") };
    },
    "POST /api/v1/statement-imports/i1/revert": { data: detail("reverted").import },
  });
}

describe("statement import", () => {
  it("uploads a file with the account's profile and opens the review", async () => {
    const seen = api();
    const { router } = renderApp("/accounts/a1/import");
    const user = userEvent.setup();
    expect(await screen.findByRole("option", { name: "Widiba" })).toBeInTheDocument();
    const file = new File(["data"], "november.xlsx");
    await user.upload(screen.getByLabelText("Statement file"), file);
    await user.click(screen.getByRole("button", { name: "Upload" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/statement-imports/i1"));
    const upload = seen.find((r) => r.method === "POST");
    const form = await upload?.formData();
    expect(form?.get("profile_id")).toBe("p1");
    expect(form?.get("account_id")).toBe("a1");
  });

  it("reviews the lines, changes one and commits, then offers to reconcile", async () => {
    const seen = api();
    renderApp("/statement-imports/i1");
    const user = userEvent.setup();
    expect(await screen.findByText("Matches a transaction")).toBeInTheDocument();
    expect(screen.getByText("same amount, 1 day apart")).toBeInTheDocument();
    expect(screen.getByText("Not on the statement")).toBeInTheDocument();
    expect(screen.getByText(/Row 21: not a transaction/)).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "New account: Bar Sport" })).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Action for 2026-11-05 Line 1"), "skip");
    await waitFor(() => expect(seen.some((r) => r.method === "PATCH")).toBe(true));
    const patch = await seen.find((r) => r.method === "PATCH")?.json();
    expect(patch).toEqual({ action: "skip" });

    await user.click(screen.getByRole("button", { name: /^Commit/ }));
    await waitFor(() => expect(seen.some((r) => r.url.endsWith("/commit"))).toBe(true));
    const reconcile = await screen.findByRole("link", { name: "Reconcile now" });
    expect(reconcile.getAttribute("href")).toContain("/accounts/a1/reconcile");
    expect(reconcile.getAttribute("href")).toContain("date=2026-11-30");
  });

  it("shows what Amazon proposes and splits a charge by item", async () => {
    const seen = api();
    renderApp("/statement-imports/i1");
    const user = userEvent.setup();
    expect(await screen.findByText("Amazon: Moka pot + 1 more")).toBeInTheDocument();
    const keep = screen.getByLabelText("Use Amazon details for 2026-11-05 AMAZON EU 405-1");
    expect(keep).toBeChecked();
    // A kept proposal names the account commit will use.
    expect(screen.getByRole("option", { name: "New account: Amazon" })).toBeInTheDocument();
    await user.click(screen.getByLabelText("Split by item for 2026-11-05 AMAZON EU 405-1"));
    await waitFor(() => expect(seen.some((r) => r.url.endsWith("/lines/2"))).toBe(true));
    const patch = await seen.find((r) => r.url.endsWith("/lines/2"))?.json();
    expect(patch).toEqual({ split_by_item: true });
    expect(
      await screen.findByLabelText("Split by item for 2026-11-05 AMAZON EU 405-1"),
    ).toBeChecked();
  });

  it("shows why a commit was refused, by row", async () => {
    fakeApi({
      "GET /api/v1/auth/me": ME,
      "GET /api/v1/household/members": MEMBERS,
      "GET /api/v1/currencies": CURRENCIES,
      "GET /api/v1/accounts": list([account("a1", "Checking", "asset")]),
      "GET /api/v1/statement-imports/i1": { data: detail("draft") },
      "POST /api/v1/statement-imports/i1/commit": json(422, {
        error: {
          code: "validation_failed",
          message: "the request has an invalid field",
          details: [{ code: "invalid", field: "lines", message: "row 3 mixes money in and out" }],
        },
      }),
    });
    renderApp("/statement-imports/i1");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Commit/ }));
    expect(await screen.findByText("row 3 mixes money in and out")).toBeInTheDocument();
  });

  it("discards a draft after asking", async () => {
    const seen = fakeApi({
      "GET /api/v1/auth/me": ME,
      "GET /api/v1/household/members": MEMBERS,
      "GET /api/v1/currencies": CURRENCIES,
      "GET /api/v1/accounts": list([account("a1", "Checking", "asset")]),
      "GET /api/v1/statement-imports/i1": { data: detail("draft") },
      "POST /api/v1/statement-imports/i1/discard": { data: detail("discarded").import },
    });
    renderApp("/statement-imports/i1");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Discard" }));
    expect(seen.some((r) => r.url.endsWith("/discard"))).toBe(false);
    await user.click(screen.getByRole("button", { name: "Really discard?" }));
    await waitFor(() => expect(seen.some((r) => r.url.endsWith("/discard"))).toBe(true));
  });

  it("warns when the file's balances do not add up", async () => {
    api();
    const { queryClient } = renderApp("/statement-imports/i1");
    await screen.findByText("Matches a transaction");
    queryClient.setQueryData(["statement-imports", "i1"], {
      ...detail("draft"),
      import: { ...detail("draft").import, gap: "-4.00" },
    });
    expect(await screen.findByText(/do not add up/)).toBeInTheDocument();
  });

  it("reverts a committed import", async () => {
    const seen = api();
    renderApp("/statement-imports/i1");
    const user = userEvent.setup();
    await screen.findByText("Matches a transaction");
    // The fake API's import starts as a draft; commit it first.
    await user.click(screen.getByRole("button", { name: /^Commit/ }));
    const revert = await screen.findByRole("button", { name: "Revert import" });
    await user.click(revert);
    await user.click(await screen.findByRole("button", { name: "Really revert?" }));
    await waitFor(() => expect(seen.some((r) => r.url.endsWith("/revert"))).toBe(true));
  });

  it("lists the account's imports on the account page", async () => {
    api();
    renderApp("/accounts/a1");
    const section = await screen.findByRole("region", { name: "Statement imports" });
    expect(within(section).getByRole("link", { name: /november\.xlsx/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Import statement" })).toBeInTheDocument();
  });
});
