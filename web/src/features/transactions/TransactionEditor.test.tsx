import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { accountsQuery, currenciesQuery, type Account } from "@/features/accounts/queries";
import { budgetsQuery } from "@/features/budgets/queries";
import { categoriesQuery } from "@/features/categories/queries";
import { tagsQuery } from "@/features/tags/queries";
import { emptySplit } from "./editor-model";
import type { TransactionRequest } from "./queries";
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
      created_at: "",
      updated_at: "",
    },
    { id: "old", name: "Old budget", active: false, sort_order: 1, created_at: "", updated_at: "" },
  ]);
  queryClient.setQueryData(tagsQuery.queryKey, [
    { id: "tag", name: "weekend", created_at: "", updated_at: "" },
  ]);
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

const row = (number: number) => screen.getByRole("group", { name: `Split ${number}` });

async function fill(number: number, source: string, destination: string, amount: string) {
  const user = userEvent.setup();
  const fields = within(row(number));
  await user.type(fields.getByLabelText("Description"), `split ${number}`);
  await user.selectOptions(fields.getByLabelText("Source account"), source);
  await user.selectOptions(fields.getByLabelText("Destination account"), destination);
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
    expect(within(row(1)).queryByRole("option", { name: "Old budget" })).toBeNull();
    await user.selectOptions(budget, "groceries");
    await user.click(screen.getByRole("button", { name: "Add split" }));
    await fill(2, "salary", "checking", "1");
    expect(within(row(2)).getByLabelText("Budget")).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Create transaction" }));

    expect(save.mock.calls[0]?.[0].splits.map((s) => s.budget_id)).toEqual(["groceries", null]);
  });

  it("flags a pair the server would reject and hides archived accounts", async () => {
    renderEditor(() => Promise.resolve({}));
    await fill(1, "food", "checking", "1");
    expect(within(row(1)).getByText("Not allowed")).toBeInTheDocument();
    expect(within(row(1)).queryByRole("option", { name: "Old (EUR)" })).toBeNull();
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

  it("removes a row but never the last one", async () => {
    renderEditor(() => Promise.resolve({}));
    const user = userEvent.setup();
    expect(screen.queryByRole("button", { name: "Remove split 1" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Add split" }));
    await user.click(screen.getByRole("button", { name: "Remove split 2" }));
    expect(screen.queryByRole("group", { name: "Split 2" })).toBeNull();
  });
});
