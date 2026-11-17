// The whole app (router, guards, pages) against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  account,
  accountRoutes,
  apiError,
  CURRENCIES,
  fakeApi,
  json,
  list,
  ME,
  MEMBERS,
  split,
} from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const ACCOUNTS = [
  account("a1", "Checking", "asset"),
  account("a2", "Food", "expense"),
  account("a3", "Old savings", "asset", { active: false }),
];
const BALANCES = [
  { account_id: "a1", balance: "-10.00", currency: "EUR" },
  { account_id: "a2", balance: "10.00", currency: "EUR" },
  { account_id: "a3", balance: "0.00", currency: "EUR" },
];
const GROUP = {
  id: "g1",
  title: "Weekend",
  created_at: "",
  updated_at: "",
  splits: [split("s1", "a1", "a2", "4.00"), split("s2", "a1", "a2", "6.00")],
};

function loggedIn(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    ...accountRoutes(ACCOUNTS, BALANCES),
    "GET /api/v1/categories": list([
      { id: "c1", name: "Groceries", created_at: "", updated_at: "" },
    ]),
    "GET /api/v1/tags": list([]),
    "GET /api/v1/budgets": list([]),
    "GET /api/v1/bills": list([]),
    "GET /api/v1/transactions": list([GROUP]),
    ...extra,
  });
}

describe("route guard", () => {
  it("sends a fresh install to setup, then into the app", async () => {
    let session = false;
    const seen = fakeApi({
      "GET /api/v1/auth/me": () => (session ? ME : apiError(401, "unauthorized")),
      "GET /api/v1/setup": () => ({ data: { needed: !session } }),
      "POST /api/v1/setup": () => json(201, { data: {} }),
      "POST /api/v1/auth/login": () => {
        session = true;
        return { data: {} };
      },
      "GET /api/v1/household/members": MEMBERS,
      "GET /api/v1/accounts": list([]),
      "GET /api/v1/accounts/balances": list([]),
      "GET /api/v1/currencies": CURRENCIES,
    });
    renderApp("/");
    const user = userEvent.setup();

    expect(await screen.findByRole("heading", { name: "Set up Saldo" })).toBeInTheDocument();
    await user.type(screen.getByLabelText("Your name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText(/^Password/), "long password");
    await user.type(screen.getByLabelText("Household name"), "Home");
    await user.click(screen.getByRole("button", { name: "Create household" }));

    expect(await screen.findByText("No accounts yet.")).toBeInTheDocument();
    const setup = seen.find((r) => r.method === "POST" && r.url.endsWith("/setup"));
    expect(await setup?.json()).toMatchObject({
      email: "ada@example.com",
      primary_currency: "EUR",
    });
  });

  it("sends a logged-out visitor to login and back to the page they wanted", async () => {
    let session = false;
    fakeApi({
      "GET /api/v1/auth/me": () => (session ? ME : apiError(401, "unauthorized")),
      "GET /api/v1/setup": { data: { needed: false } },
      "POST /api/v1/auth/login": async (request: Request) => {
        const body = (await request.json()) as { password: string };
        if (body.password !== "right")
          return apiError(401, "unauthorized", "wrong email or password");
        session = true;
        return { data: {} };
      },
      "GET /api/v1/household/members": MEMBERS,
      "GET /api/v1/categories": list([]),
      "GET /api/v1/budgets": list([]),
      "GET /api/v1/bills": list([]),
    });
    const { router } = renderApp("/categories");
    const user = userEvent.setup();

    expect(await screen.findByRole("heading", { name: "Log in" })).toBeInTheDocument();
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "wrong");
    await user.click(screen.getByRole("button", { name: "Log in" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("wrong email or password");

    await user.clear(screen.getByLabelText("Password"));
    await user.type(screen.getByLabelText("Password"), "right");
    await user.click(screen.getByRole("button", { name: "Log in" }));
    expect(await screen.findByRole("heading", { name: "Categories" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/categories");
  });
});

describe("accounts", () => {
  it("lists active accounts by type with balances and totals", async () => {
    loggedIn();
    renderApp("/accounts");
    const user = userEvent.setup();

    const assets = await screen.findByTestId("group-asset");
    expect(await within(assets).findByText("Checking")).toBeInTheDocument();
    expect(within(assets).getAllByText("-€10.00")).toHaveLength(2);
    expect(within(assets).queryByText("Old savings")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Show archived" }));
    expect(await within(assets).findByText("Old savings")).toBeInTheDocument();
  });

  it("loads expense accounts only when their section is opened, a page at a time", async () => {
    const payees = Array.from({ length: 60 }, (_, i) =>
      account(`p${i}`, `Payee ${String(i).padStart(2, "0")}`, "expense"),
    );
    const seen = loggedIn(accountRoutes([...ACCOUNTS, ...payees], BALANCES));
    renderApp("/accounts");
    const user = userEvent.setup();

    await screen.findByTestId("group-asset");
    const listed = (type: string) =>
      seen.filter((r) => r.url.includes("/accounts?") && r.url.includes(`type=${type}`));
    expect(listed("asset").length).toBeGreaterThan(0);
    expect(listed("expense")).toHaveLength(0);
    expect(screen.queryByText("Payee 00")).toBeNull();

    await user.click(screen.getByRole("button", { name: /Expense accounts/ }));
    const expenses = await screen.findByTestId("group-expense");
    expect(await within(expenses).findByText("Food")).toBeInTheDocument();
    expect(within(expenses).queryByText("Payee 59")).toBeNull();
    expect(within(expenses).getByText("50 of 61")).toBeInTheDocument();

    await user.click(within(expenses).getByRole("button", { name: "Load more" }));
    expect(await within(expenses).findByText("Payee 59")).toBeInTheDocument();
    expect(within(expenses).queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("searches every type by name on the server", async () => {
    const seen = loggedIn();
    renderApp("/accounts");
    const user = userEvent.setup();

    await screen.findByTestId("group-asset");
    await user.type(screen.getByRole("searchbox", { name: "Search accounts" }), "foo");
    const food = await screen.findByText("Food");
    expect(screen.getByTestId("group-expense")).toContainElement(food);
    expect(screen.queryByTestId("group-asset")).toBeNull();
    expect(seen.some((r) => r.url.includes("/accounts?") && r.url.includes("q=foo"))).toBe(true);

    await user.type(screen.getByRole("searchbox", { name: "Search accounts" }), "zzz");
    expect(await screen.findByText("No accounts match “foozzz”.")).toBeInTheDocument();
  });

  it("keeps the scope switch in the URL and sends it to the API", async () => {
    const seen = loggedIn();
    const { router } = renderApp("/accounts");
    const user = userEvent.setup();

    await screen.findByTestId("group-asset");
    await user.selectOptions(screen.getByLabelText("Scope"), "mine");
    await waitFor(() => expect(router.state.location.search).toMatchObject({ scope: "mine" }));
    await waitFor(() =>
      expect(seen.some((r) => r.url.includes("/accounts?") && r.url.includes("scope=mine"))).toBe(
        true,
      ),
    );
    const nav = screen.getByRole("navigation", { name: "Main" });
    await user.click(within(nav).getByRole("button", { name: "More" }));
    await user.click(within(nav).getByRole("link", { name: "Categories" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/categories"));
    expect(router.state.location.search).toMatchObject({ scope: "mine" });
  });

  it("shows an account with its balance and archives it", async () => {
    const seen = loggedIn({
      "GET /api/v1/accounts/a1": { data: ACCOUNTS[0] },
      "GET /api/v1/accounts/a1/balance": {
        data: { account_id: "a1", balance: "-10.00", currency: "EUR" },
      },
      "PATCH /api/v1/accounts/a1": { data: { ...ACCOUNTS[0], active: false } },
    });
    renderApp("/accounts/a1");
    const user = userEvent.setup();

    expect(await screen.findByTestId("account-balance")).toHaveTextContent("-€10.00");
    expect(await screen.findByText("Weekend")).toBeInTheDocument();
    expect(screen.getAllByText("-€4.00")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: /Archive/ }));
    await waitFor(() => expect(seen.some((r) => r.method === "PATCH")).toBe(true));
    const patch = seen.find((r) => r.method === "PATCH");
    expect(await patch?.json()).toEqual({ active: false });
  });

  it("creates an account and opens it", async () => {
    const seen = loggedIn({
      "POST /api/v1/accounts": () => json(201, { data: account("a9", "Wallet", "asset") }),
      "GET /api/v1/accounts/a9": { data: account("a9", "Wallet", "asset") },
      "GET /api/v1/accounts/a9/balance": {
        data: { account_id: "a9", balance: "0.00", currency: "EUR" },
      },
    });
    const { router } = renderApp("/accounts/new");
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText("Name"), "Wallet");
    await user.selectOptions(screen.getByLabelText("Role"), "cash_wallet");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    await waitFor(() => expect(router.state.location.pathname).toBe("/accounts/a9"));
    const post = seen.find((r) => r.method === "POST");
    expect(await post?.json()).toMatchObject({
      name: "Wallet",
      type: "asset",
      asset_role: "cash_wallet",
    });
  });
});

describe("transactions", () => {
  it("shows split groups expanded and puts filters in the URL", async () => {
    const seen = loggedIn();
    const { router } = renderApp("/transactions?account_id=0190f5c3-7a2b-7c00-8000-000000000001");
    const user = userEvent.setup();

    expect(await screen.findByText("(2 splits)")).toBeInTheDocument();
    expect(screen.getByText("split s1")).toBeInTheDocument();
    expect(screen.getByText("split s2")).toBeInTheDocument();
    // The account filter is set, so the folded panel says one filter is active.
    const filters = screen.getByRole("button", { name: /^Filters\s*1\s*active$/ });
    expect(screen.getByLabelText("Kind")).not.toBeVisible();
    await user.click(filters);
    expect(filters).toHaveAttribute("aria-expanded", "true");
    await user.selectOptions(screen.getByLabelText("Kind"), "transfer");
    await waitFor(() => expect(router.state.location.search).toMatchObject({ kind: "transfer" }));
    await user.type(screen.getByLabelText("Search"), "bread");
    await user.click(screen.getByRole("button", { name: "Search" }));
    await waitFor(() =>
      expect(
        seen.some(
          (r) =>
            r.url.includes("kind=transfer") &&
            r.url.includes("q=bread") &&
            r.url.includes("account_id=0190f5c3"),
        ),
      ).toBe(true),
    );
  });

  it("lists days with totals and folds upcoming transactions away", async () => {
    const future = {
      id: "g2",
      title: null,
      created_at: "",
      updated_at: "",
      splits: [{ ...split("s3", "a1", "a2", "9.00"), date: "2999-01-01" }],
    };
    loggedIn({ "GET /api/v1/transactions": list([future, GROUP]) });
    renderApp("/transactions");
    const user = userEvent.setup();

    const [day] = await screen.findAllByTestId("day-heading");
    if (!day) throw new Error("expected a day heading");
    expect(within(day).getByText("-€10.00")).toBeInTheDocument();
    expect(screen.getByTestId("group-total")).toHaveTextContent("-€10.00");
    expect(screen.queryByText("split s3")).not.toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: /Upcoming/ });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    const row = screen.getByText("split s3").closest("tr");
    if (!row) throw new Error("expected a row");
    expect(row).toHaveAttribute("data-future", "true");
    expect(within(row).getAllByText("Withdrawal")[0]).toHaveClass("text-kind-withdrawal");
    expect(within(row).getByText("-€9.00")).toHaveClass("text-destructive");
  });

  it("loads a group into the editor and deletes it", async () => {
    const seen = loggedIn({
      "GET /api/v1/transactions/g1": { data: GROUP },
      "DELETE /api/v1/transactions/g1": () => new Response(null, { status: 204 }),
    });
    const { router } = renderApp("/transactions/g1/edit");
    const user = userEvent.setup();

    expect(await screen.findByDisplayValue("Weekend")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Split 2" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await user.click(screen.getByRole("button", { name: "Really delete?" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/transactions"));
    expect(seen.some((r) => r.method === "DELETE")).toBe(true);
  });

  it("shows one attachments panel per saved split", async () => {
    const seen = loggedIn({
      "GET /api/v1/transactions/g1": { data: GROUP },
      "GET /api/v1/attachments": { data: [] },
    });
    renderApp("/transactions/g1/edit");

    expect(await screen.findByText("Attachments · split 1: split s1")).toBeInTheDocument();
    expect(screen.getByText("Attachments · split 2: split s2")).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByText("No files yet.")).toHaveLength(2));
    const targets = seen
      .filter((r) => new URL(r.url).pathname === "/api/v1/attachments")
      .map((r) => new URL(r.url).searchParams.get("attachable_id"));
    expect(targets.sort()).toEqual(["s1", "s2"]);
  });
});

describe("categories and tags", () => {
  it("gives a category a colour from the palette and shows it in the list", async () => {
    const seen = loggedIn({
      "GET /api/v1/categories": list([
        { id: "c1", name: "Groceries", color: "green", created_at: "", updated_at: "" },
      ]),
      "PATCH /api/v1/categories/c1": { data: {} },
    });
    renderApp("/categories");
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Colour of Groceries" }));
    const palette = await screen.findByRole("group", { name: "Colours" });
    expect(within(palette).getByRole("button", { name: "green" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await user.click(within(palette).getByRole("button", { name: "blue" }));
    await waitFor(() => expect(seen.some((r) => r.method === "PATCH")).toBe(true));
    const patch = seen.find((r) => r.method === "PATCH");
    expect(await patch?.json()).toEqual({ color: "blue" });
  });

  it("shows a category's colour beside it in the transaction list", async () => {
    const coloured = {
      ...GROUP,
      splits: [{ ...split("s1", "a1", "a2", "4.00"), category_id: "c1" }],
    };
    loggedIn({
      "GET /api/v1/categories": list([
        { id: "c1", name: "Groceries", color: "green", created_at: "", updated_at: "" },
      ]),
      "GET /api/v1/transactions": list([coloured]),
    });
    renderApp("/transactions");

    const row = (await screen.findAllByTestId("transaction-row"))[0];
    if (!row) throw new Error("expected a row");
    await within(row).findByText("Groceries");
    expect(row.querySelector('[data-color="green"]')).toHaveClass("bg-cat-green");
  });

  it("adds, renames and deletes a tag", async () => {
    let tags = [{ id: "t1", name: "weekend", created_at: "", updated_at: "" }];
    const seen = loggedIn({
      "GET /api/v1/tags": () => list(tags),
      "POST /api/v1/tags": async (request: Request) => {
        const { name } = (await request.json()) as { name: string };
        tags = [...tags, { id: "t2", name, created_at: "", updated_at: "" }];
        return json(201, { data: tags[1] });
      },
      "PATCH /api/v1/tags/t1": { data: {} },
      "DELETE /api/v1/tags/t1": () => new Response(null, { status: 204 }),
    });
    renderApp("/tags");
    const user = userEvent.setup();

    await screen.findByText("weekend");
    await user.type(screen.getByLabelText("New tag name"), "holiday");
    await user.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByText("holiday")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Rename weekend" }));
    const input = screen.getByLabelText("tag name");
    await user.clear(input);
    await user.type(input, "weekends");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await user.click(screen.getByRole("button", { name: "Delete weekend" }));
    await waitFor(() => expect(seen.some((r) => r.method === "DELETE")).toBe(true));
    const patch = seen.find((r) => r.method === "PATCH");
    expect(await patch?.json()).toEqual({ name: "weekends" });
  });

  it("shows a conflict from the server", async () => {
    loggedIn({
      "GET /api/v1/categories": list([]),
      "GET /api/v1/budgets": list([]),
      "GET /api/v1/bills": list([]),
      "POST /api/v1/categories": apiError(409, "conflict", "a category with this name exists"),
    });
    renderApp("/categories");
    const user = userEvent.setup();

    await screen.findByText("None yet.");
    await user.type(screen.getByLabelText("New category name"), "Food");
    await user.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("a category with this name exists");
  });
});

describe("shell", () => {
  it("logs out and clears the session", async () => {
    let session = true;
    const seen = loggedIn({
      "GET /api/v1/auth/me": () => (session ? ME : apiError(401, "unauthorized")),
      "GET /api/v1/setup": { data: { needed: false } },
      "POST /api/v1/auth/logout": () => {
        session = false;
        return new Response(null, { status: 204 });
      },
    });
    const { router } = renderApp("/accounts");
    const user = userEvent.setup();

    await screen.findByTestId("group-asset");
    await user.click(screen.getByRole("button", { name: "Log out" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/login"));
    expect(seen.some((r) => r.url.endsWith("/auth/logout"))).toBe(true);
  });
});

describe("home", () => {
  it("opens on the transaction list, which leads the navigation", async () => {
    loggedIn();
    const { router } = renderApp("/");

    await waitFor(() => expect(router.state.location.pathname).toBe("/transactions"));
    const nav = screen.getByRole("navigation", { name: "Main" });
    expect(within(nav).getAllByRole("link")[0]).toHaveTextContent("Transactions");
    expect(screen.getByRole("link", { name: "Saldo" })).toHaveAttribute("href", "/transactions");
  });
});
