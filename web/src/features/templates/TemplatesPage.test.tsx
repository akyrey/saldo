import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, json, list, ME, MEMBERS, split } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => vi.unstubAllGlobals());

const T1 = "0190f5c3-7a2b-7c00-8000-0000000000a1";
const T2 = "0190f5c3-7a2b-7c00-8000-0000000000a2";
const G1 = "0190f5c3-7a2b-7c00-8000-0000000000b1";

function template(id: string, name: string, extra: object = {}) {
  return {
    id,
    name,
    kind: "withdrawal",
    title: null,
    splits: [],
    sort_order: 0,
    created_at: "",
    updated_at: "",
    ...extra,
  };
}

const COFFEE = template(T1, "Coffee", {
  sort_order: 0,
  splits: [
    { description: "Coffee", source_id: "a1", amount: "2.50", currency: "EUR", tag_ids: [] },
  ],
});
const RENT = template(T2, "Rent", { sort_order: 1, kind: "transfer" });
const GROUP = {
  id: G1,
  title: null,
  created_at: "",
  updated_at: "",
  splits: [{ ...split("s1", "a1", "a2", "3.20"), date: "2026-10-01", reconciled: true }],
};

function api(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([
      account("a1", "Checking", "asset"),
      account("a2", "Food", "expense"),
      account("a3", "Salary", "revenue"),
    ]),
    "GET /api/v1/categories": list([]),
    "GET /api/v1/tags": list([]),
    "GET /api/v1/budgets": list([]),
    "GET /api/v1/transactions": list([GROUP]),
    "GET /api/v1/transaction-templates": list([COFFEE, RENT]),
    [`GET /api/v1/transaction-templates/${T1}`]: { data: COFFEE },
    [`GET /api/v1/transaction-templates/${T2}`]: { data: RENT },
    [`GET /api/v1/transactions/${G1}`]: { data: GROUP },
    ...extra,
  });
}

async function bodiesOf(seen: Request[], method: string, path: string) {
  const requests = seen.filter((r) => r.method === method && new URL(r.url).pathname === path);
  return Promise.all(requests.map((r) => r.clone().json() as Promise<Record<string, unknown>>));
}

describe("templates page", () => {
  it("lists templates, reorders, renames and deletes them", async () => {
    const seen = api({
      [`PATCH /api/v1/transaction-templates/${T1}`]: { data: COFFEE },
      [`PATCH /api/v1/transaction-templates/${T2}`]: { data: RENT },
      [`DELETE /api/v1/transaction-templates/${T2}`]: () => new Response(null, { status: 204 }),
    });
    renderApp("/templates");
    const user = userEvent.setup();

    expect(await screen.findByText("Coffee · 2.50 EUR")).toBeInTheDocument();
    expect(screen.getByText("Transfer")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Move Rent up" }));
    await waitFor(async () =>
      expect(await bodiesOf(seen, "PATCH", `/api/v1/transaction-templates/${T2}`)).toEqual([
        { sort_order: 0 },
      ]),
    );
    expect(await bodiesOf(seen, "PATCH", `/api/v1/transaction-templates/${T1}`)).toEqual([
      { sort_order: 1 },
    ]);

    await user.click(screen.getByRole("button", { name: "Rename Coffee" }));
    const input = screen.getByLabelText("Template name");
    await user.clear(input);
    await user.type(input, "Espresso");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await user.click(screen.getByRole("button", { name: "Delete Rent" }));
    await user.click(screen.getByRole("button", { name: "Really delete Rent?" }));
    await waitFor(() => expect(seen.some((r) => r.method === "DELETE")).toBe(true));
    const patches = await bodiesOf(seen, "PATCH", `/api/v1/transaction-templates/${T1}`);
    expect(patches).toContainEqual({ name: "Espresso" });
  });
});

describe("using templates in the editor", () => {
  it("fills the editor from a template and posts a normal transaction", async () => {
    const seen = api({ "POST /api/v1/transactions": () => json(201, { data: GROUP }) });
    const { router } = renderApp(`/transactions/new?template=${T1}`);
    const user = userEvent.setup();

    expect(await screen.findByRole("heading", { name: "New from “Coffee”" })).toBeInTheDocument();
    const row = await screen.findByRole("group", { name: "Split 1" });
    expect(within(row).getByLabelText("Description")).toHaveValue("Coffee");
    // A withdrawal template offers only expense accounts as destination.
    const destination = within(row).getByLabelText("Destination account");
    expect(within(destination).queryByText(/Salary/)).toBeNull();
    await user.selectOptions(destination, "a2");
    const amount = within(row).getByLabelText(/^Amount/);
    await user.clear(amount);
    await user.type(amount, "3");
    await user.click(screen.getByRole("button", { name: "Create transaction" }));

    await waitFor(() => expect(router.state.location.pathname).toBe("/transactions"));
    const [body] = await bodiesOf(seen, "POST", "/api/v1/transactions");
    const splits = body?.splits as Record<string, unknown>[];
    expect(splits[0]).toMatchObject({ description: "Coffee", destination_id: "a2", amount: "3" });
  });

  it("switches templates with the picker", async () => {
    api();
    const { router } = renderApp("/transactions");
    const user = userEvent.setup();
    await user.selectOptions(await screen.findByLabelText("New from template"), T2);
    await waitFor(() => expect(router.state.location.search).toMatchObject({ template: T2 }));
    expect(await screen.findByRole("heading", { name: "New from “Rent”" })).toBeInTheDocument();
  });

  it("saves the form as a template without a date", async () => {
    const seen = api({
      "POST /api/v1/transaction-templates": () => json(201, { data: COFFEE }),
    });
    renderApp("/transactions/new");
    const user = userEvent.setup();
    const row = await screen.findByRole("group", { name: "Split 1" });
    await user.type(within(row).getByLabelText("Description"), "Tea");
    await user.click(screen.getByRole("button", { name: "Save as template" }));
    await user.type(screen.getByLabelText("Template name"), "Tea");
    await user.click(screen.getByRole("button", { name: "Save template" }));

    expect(await screen.findByRole("status")).toHaveTextContent("Template saved");
    const [body] = await bodiesOf(seen, "POST", "/api/v1/transaction-templates");
    expect(body).toMatchObject({ name: "Tea", kind: "withdrawal" });
    const splits = body?.splits as Record<string, unknown>[];
    expect(splits[0]).toMatchObject({ description: "Tea", amount: null });
    expect(splits[0]).not.toHaveProperty("date");
  });

  it("duplicates a transaction with today's date", async () => {
    api();
    renderApp(`/transactions/${G1}/edit`);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("link", { name: /Duplicate/ }));
    expect(
      await screen.findByRole("heading", { name: "Duplicate transaction" }),
    ).toBeInTheDocument();
    const row = screen.getByRole("group", { name: "Split 1" });
    expect(within(row).getByLabelText("Amount (EUR)")).toHaveValue("3.20");
    expect(within(row).getByLabelText("Date")).not.toHaveValue("2026-10-01");
  });

  it("saves an existing transaction as a template", async () => {
    const seen = api({
      [`POST /api/v1/transactions/${G1}/template`]: () => json(201, { data: COFFEE }),
    });
    renderApp(`/transactions/${G1}/edit`);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Save as template" }));
    const name = screen.getByLabelText("Template name");
    expect(name).toHaveValue("split s1");
    await user.click(screen.getByRole("button", { name: "Save template" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Template saved");
    const [body] = await bodiesOf(seen, "POST", `/api/v1/transactions/${G1}/template`);
    expect(body).toEqual({ name: "split s1" });
  });
});
