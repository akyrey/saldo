import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { choose } from "@/test/combobox";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { accountsQuery, currenciesQuery, type Account } from "@/features/accounts/queries";
import { billsQuery, type Bill } from "@/features/bills/queries";
import { budgetsQuery } from "@/features/budgets/queries";
import { categoriesQuery } from "@/features/categories/queries";
import { tagsQuery } from "@/features/tags/queries";
import { emptySplit } from "./editor-model";
import { descriptionSearchQuery, type TransactionRequest } from "./queries";
import { split } from "@/test/fake-api";
import { TransactionEditor } from "./TransactionEditor";

function account(id: string, name: string, type: string): Account {
  return {
    id,
    name,
    type,
    currency: "EUR",
    virtual_balance: "0.00",
    include_net_worth: true,
    active: true,
    sort_order: 0,
    created_at: "",
    updated_at: "",
  };
}

function bill(id: string, name: string, active: boolean): Bill {
  return {
    id,
    name,
    active,
    amount_min: "10.00",
    amount_max: "20.00",
    currency: "EUR",
    first_due_date: "2026-01-01",
    repeat_freq: "monthly",
    skip: 0,
    remind_days_before: 0,
    remind_every_days: 0,
    sort_order: 0,
    created_at: "",
    updated_at: "",
  };
}

const ACCOUNTS = [
  account("checking", "Checking", "asset"),
  account("card", "Card", "liability"),
  account("food", "Food", "expense"),
  account("salary", "Salary", "revenue"),
  { ...account("old", "Old", "asset"), active: false },
];

function renderEditor(save: (body: TransactionRequest) => Promise<unknown>) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
  queryClient.setQueryData(accountsQuery().queryKey, ACCOUNTS);
  queryClient.setQueryData(categoriesQuery.queryKey, []);
  queryClient.setQueryData(budgetsQuery.queryKey, [
    {
      id: "groceries",
      name: "Groceries",
      active: true,
      sort_order: 0,
      carry: "none",
      created_at: "",
      updated_at: "",
    },
    {
      id: "old",
      name: "Old budget",
      active: false,
      sort_order: 1,
      carry: "none",
      created_at: "",
      updated_at: "",
    },
  ]);
  queryClient.setQueryData(billsQuery.queryKey, [
    bill("rent", "Rent", true),
    bill("gym", "Gym", false),
  ]);
  queryClient.setQueryData(tagsQuery.queryKey, [
    { id: "tag", name: "weekend", created_at: "", updated_at: "" },
  ]);
  queryClient.setQueryData(descriptionSearchQuery("Gro").queryKey, {
    data: [
      {
        id: "past",
        title: null,
        created_at: "",
        updated_at: "",
        splits: [{ ...split("s1", "checking", "food", "3.00"), description: "Groceries" }],
      },
    ],
    meta: { total: 1, page: 1, per_page: 50 },
  });
  queryClient.setQueryData(currenciesQuery.queryKey, [
    { code: "EUR", name: "Euro", symbol: "€", decimal_places: 2 },
  ]);
  const onSaved = vi.fn();
  render(
    <QueryClientProvider client={queryClient}>
      <TransactionEditor
        initial={{ title: "", splits: [emptySplit("2026-11-07")] }}
        submitLabel="Create transaction"
        save={save}
        onSaved={onSaved}
      />
    </QueryClientProvider>,
  );
  return { onSaved };
}

/** How the account pickers show an account: `Checking (EUR)`. */
const accountLabel = (id: string) => `${ACCOUNTS.find((a) => a.id === id)?.name ?? id} (EUR)`;

const row = (number: number) => screen.getByRole("group", { name: `Split ${number}` });

async function fill(number: number, source: string, destination: string, amount: string) {
  const user = userEvent.setup();
  const fields = within(row(number));
  await user.type(fields.getByLabelText("Description"), `split ${number}`);
  await choose(fields.getByLabelText("Source account"), accountLabel(source));
  await choose(fields.getByLabelText("Destination account"), accountLabel(destination));
  await user.type(fields.getByLabelText(/^Amount/), amount);
}

describe("TransactionEditor", () => {
  it("shows each row's kind and the exact total, then sends every split", async () => {
    const save = vi.fn((_body: TransactionRequest) => Promise.resolve({}));
    const { onSaved } = renderEditor(save);
    const user = userEvent.setup();

    await fill(1, "checking", "food", "0.10");
    expect(within(row(1)).getByText("Withdrawal")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add split" }));
    await fill(2, "salary", "card", "0,20");
    expect(within(row(2)).getByText("Deposit")).toBeInTheDocument();
    await user.type(within(row(2)).getByRole("combobox", { name: "Add tag" }), "week");
    await user.click(within(row(2)).getByRole("option", { name: "weekend" }));
    expect(screen.getByTestId("editor-total")).toHaveTextContent("Total: 0.30 EUR");

    await user.click(screen.getByRole("button", { name: "Create transaction" }));

    expect(onSaved).toHaveBeenCalled();
    const body = save.mock.calls[0]?.[0];
    expect(body?.splits.map((s) => [s.source_id, s.destination_id, s.amount, s.tag_ids])).toEqual([
      ["checking", "food", "0.10", []],
      ["salary", "card", "0.20", ["tag"]],
    ]);
  });

  it("offers active budgets on withdrawals only", async () => {
    const save = vi.fn((_body: TransactionRequest) => Promise.resolve({}));
    renderEditor(save);
    const user = userEvent.setup();
    await fill(1, "checking", "food", "5");
    const budget = within(row(1)).getByLabelText("Budget");
    await user.type(budget, "old");
    expect(within(row(1)).queryByRole("option", { name: "Old budget" })).toBeNull();
    await choose(budget, "Groceries");
    await user.click(screen.getByRole("button", { name: "Add split" }));
    await fill(2, "salary", "checking", "1");
    expect(within(row(2)).getByLabelText("Budget")).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Create transaction" }));

    expect(save.mock.calls[0]?.[0].splits.map((s) => s.budget_id)).toEqual(["groceries", null]);
  });

  it("links a split to an active bill, not an archived one", async () => {
    const save = vi.fn((_body: TransactionRequest) => Promise.resolve({}));
    renderEditor(save);
    const user = userEvent.setup();
    await fill(1, "checking", "food", "15");
    const billInput = within(row(1)).getByLabelText("Bill");
    await user.type(billInput, "gym");
    expect(within(row(1)).queryByRole("option", { name: "Gym" })).toBeNull();
    await choose(billInput, "Rent");
    await user.click(screen.getByRole("button", { name: "Add split" }));
    await fill(2, "checking", "food", "1");

    await user.click(screen.getByRole("button", { name: "Create transaction" }));

    expect(save.mock.calls[0]?.[0].splits.map((s) => s.bill_id)).toEqual(["rent", null]);
  });

  it("offers only accounts that fit the other side, and no archived ones", async () => {
    renderEditor(() => Promise.resolve({}));
    const user = userEvent.setup();
    const fields = within(row(1));
    const offered = async (label: string) => {
      const field = fields.getByLabelText(label);
      await user.clear(field);
      await user.click(field);
      return screen.queryAllByRole("option").map((option) => option.textContent ?? "");
    };

    // Money never leaves an expense account, and archived accounts stay hidden.
    const sources = await offered("Source account");
    expect(sources.some((o) => o.includes("Food"))).toBe(false);
    expect(sources.some((o) => o.includes("Old"))).toBe(false);
    expect(sources.some((o) => o.includes("Salary"))).toBe(true);

    // Revenue only pays into own accounts.
    await choose(fields.getByLabelText("Source account"), accountLabel("salary"));
    const destinations = await offered("Destination account");
    expect(destinations.some((o) => o.includes("Food"))).toBe(false);
    expect(destinations.some((o) => o.includes("Checking"))).toBe(true);
  });

  it("puts server validation errors on the matching row and field", async () => {
    const error = new ApiError(422, "validation_failed", "invalid", [
      { field: "splits[1].amount", code: "amount_not_positive", message: "must be positive" },
      { field: "splits[0]", code: "invalid_account_pair", message: "bad pair" },
    ]);
    const { onSaved } = renderEditor(() => Promise.reject(error));
    const user = userEvent.setup();
    await fill(1, "checking", "food", "5");
    await user.click(screen.getByRole("button", { name: "Add split" }));
    await fill(2, "card", "food", "0");

    await user.click(screen.getByRole("button", { name: "Create transaction" }));

    const secondAmount = within(row(2)).getByLabelText(/^Amount/);
    expect(await within(row(2)).findByText("must be positive")).toBeInTheDocument();
    expect(secondAmount).toHaveAttribute("aria-invalid", "true");
    expect(within(row(1)).getByLabelText(/^Amount/)).not.toHaveAttribute("aria-invalid");
    expect(within(row(1)).getByText("bad pair")).toBeInTheDocument();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("shows a general message when the request fails without details", async () => {
    renderEditor(() => Promise.reject(new ApiError(0, "network", "offline")));
    await fill(1, "checking", "food", "5");
    await userEvent.setup().click(screen.getByRole("button", { name: "Create transaction" }));
    expect(await screen.findByText(/Cannot reach the server/)).toBeInTheDocument();
  });

  it("suggests descriptions used before", async () => {
    const user = userEvent.setup();
    renderEditor(vi.fn());
    const field = screen.getByLabelText("Description");
    await user.type(field, "Gro");
    await user.click(await screen.findByRole("option", { name: "Groceries" }));
    expect(field).toHaveValue("Groceries");
  });

  it("removes a row but never the last one", async () => {
    renderEditor(() => Promise.resolve({}));
    const user = userEvent.setup();
    expect(screen.queryByRole("button", { name: "Remove split 1" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Add split" }));
    await user.click(screen.getByRole("button", { name: "Remove split 2" }));
    expect(screen.queryByRole("group", { name: "Split 2" })).toBeNull();
  });
});
