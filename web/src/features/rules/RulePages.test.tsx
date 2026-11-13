// The rule pages inside the whole app, against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { choose } from "@/test/combobox";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const GROUP = {
  id: "g1",
  title: "Default",
  description: null,
  sort_order: 0,
  active: true,
  created_at: "",
  updated_at: "",
};

function rule(id: string, title: string, order: number) {
  return {
    id,
    rule_group_id: "g1",
    title,
    description: null,
    trigger_on: "create",
    strict: true,
    stop_processing: false,
    active: true,
    sort_order: order,
    triggers: [{ kind: "description_contains", value: title.toLowerCase(), negate: false }],
    actions: [{ kind: "set_category", value: "c1", stop_processing: false, active: true }],
    created_at: "",
    updated_at: "",
  };
}

function api() {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([account("a1", "Checking", "asset")]),
    "GET /api/v1/categories": list([{ id: "c1", name: "Housing", created_at: "", updated_at: "" }]),
    "GET /api/v1/tags": list([]),
    "GET /api/v1/bills": list([]),
    "GET /api/v1/budgets": list([
      {
        id: "b1",
        name: "Groceries",
        active: true,
        sort_order: 0,
        notes: null,
        auto_budget: null,
        created_at: "",
        updated_at: "",
      },
    ]),
    "GET /api/v1/rule-groups": { data: [GROUP] },
    "GET /api/v1/rules": { data: [rule("r1", "Rent", 0), rule("r2", "Fuel", 1)] },
    "GET /api/v1/rules/r1": { data: rule("r1", "Rent", 0) },
    "PUT /api/v1/rules/r1": { data: rule("r1", "Rent", 1) },
    "PUT /api/v1/rules/r2": { data: rule("r2", "Fuel", 0) },
    "POST /api/v1/rules": () =>
      new Response(JSON.stringify({ data: rule("r3", "New", 2) }), {
        status: 201,
        headers: { "Content-Type": "application/json" },
      }),
    "POST /api/v1/rules/r1/test": {
      data: {
        total: 1,
        matches: [
          {
            group_id: "t1",
            split_id: "s1",
            date: "2026-11-02",
            description: "Rent November",
            amount: "800.00",
            currency: "EUR",
          },
        ],
      },
    },
  });
}

describe("rule pages", () => {
  it("lists rules by group and moves one down", async () => {
    const seen = api();
    renderApp("/rules");
    const group = await screen.findByRole("region", { name: "Default" });
    const items = within(group).getAllByRole("listitem");
    expect(items.map((i) => within(i).getByRole("link").textContent)).toEqual(["Rent", "Fuel"]);
    await userEvent.click(
      within(items[0] as HTMLElement).getByRole("button", { name: "Move Rent down" }),
    );
    await waitFor(() => expect(seen.filter((r) => r.method === "PUT")).toHaveLength(2));
    const put = seen.find((r) => new URL(r.url).pathname === "/api/v1/rules/r1");
    expect(((await put?.json()) as { sort_order: number }).sort_order).toBe(1);
  });

  it("creates a rule from the editor", async () => {
    const seen = api();
    renderApp("/rules/new?group=g1");
    await userEvent.type(await screen.findByLabelText("Title"), "Groceries");
    await userEvent.type(screen.getByLabelText("Trigger 1 value"), "supermarket");
    await choose(screen.getByLabelText("Action 1 value"), "Housing");
    await userEvent.click(screen.getByRole("button", { name: "Create rule" }));
    await waitFor(() =>
      expect(
        seen.some((r) => r.method === "POST" && new URL(r.url).pathname === "/api/v1/rules"),
      ).toBe(true),
    );
    const post = seen.find(
      (r) => r.method === "POST" && new URL(r.url).pathname === "/api/v1/rules",
    );
    expect(await post?.json()).toMatchObject({
      rule_group_id: "g1",
      title: "Groceries",
      triggers: [{ kind: "description_contains", value: "supermarket", negate: false }],
      actions: [{ kind: "set_category", value: "c1" }],
    });
  });

  it("picks a budget by name for a set_budget action", async () => {
    const seen = api();
    renderApp("/rules/new?group=g1");
    await userEvent.type(await screen.findByLabelText("Title"), "Food");
    await userEvent.type(screen.getByLabelText("Trigger 1 value"), "market");
    await choose(screen.getByLabelText("Action 1 kind"), "Set budget", "budget");
    await choose(screen.getByLabelText("Action 1 value"), "Groceries", "groc");
    await userEvent.click(screen.getByRole("button", { name: "Create rule" }));
    await waitFor(() =>
      expect(seen.some((r) => r.method === "POST" && r.url.endsWith("/api/v1/rules"))).toBe(true),
    );
    const post = seen.find((r) => r.method === "POST" && r.url.endsWith("/api/v1/rules"));
    expect(await post?.json()).toMatchObject({ actions: [{ kind: "set_budget", value: "b1" }] });
  });

  it("previews the transactions a saved rule matches", async () => {
    api();
    renderApp("/rules/r1");
    await userEvent.click(await screen.findByRole("button", { name: "Test rule" }));
    expect(await screen.findByText("Rent November")).toBeInTheDocument();
    expect(screen.getByText(/1 matching split/)).toBeInTheDocument();
  });
});
