import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { accountsQuery, type Account } from "@/features/accounts/queries";
import { apiError, fakeApi } from "@/test/fake-api";
import { AccountSelect } from "./AccountSelect";

afterEach(() => {
  vi.unstubAllGlobals();
});

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

const ACCOUNTS = [account("checking", "Checking", "asset"), account("food", "Food", "expense")];

function Harness() {
  const [value, setValue] = useState("");
  return (
    <>
      <label htmlFor="destination">Destination account</label>
      <AccountSelect
        id="destination"
        accounts={ACCOUNTS}
        types={["asset", "expense"]}
        value={value}
        onChange={setValue}
        create={{ type: "expense", currency: "EUR" }}
      />
      <output data-testid="value">{value}</output>
    </>
  );
}

function renderSelect() {
  const queryClient = new QueryClient();
  queryClient.setQueryData(accountsQuery().queryKey, ACCOUNTS);
  render(
    <QueryClientProvider client={queryClient}>
      <Harness />
    </QueryClientProvider>,
  );
  return queryClient;
}

const field = () => screen.getByLabelText("Destination account");
const value = () => screen.getByTestId("value").textContent;

describe("AccountSelect", () => {
  it("creates an expense account from the typed name and chooses it", async () => {
    const seen = fakeApi({
      "POST /api/v1/accounts": () =>
        Response.json({ data: account("gym", "Gym", "expense") }, { status: 201 }),
    });
    const queryClient = renderSelect();
    await userEvent.type(field(), " Gym ");
    await userEvent.click(screen.getByRole("option", { name: "Create expense account “Gym”" }));
    await waitFor(() => expect(value()).toBe("gym"));
    const post = seen.find((r) => r.method === "POST");
    expect(await post?.json()).toEqual({ name: "Gym", type: "expense", currency: "EUR" });
    const cached = queryClient.getQueryData<Account[]>(accountsQuery().queryKey);
    expect(cached?.map((a) => a.id)).toContain("gym");
  });

  it("does not offer to create a name that already exists for that type", async () => {
    renderSelect();
    await userEvent.type(field(), "food");
    expect(screen.queryByText(/Create expense account/)).toBeNull();
  });

  it("shows why the server refused the new account", async () => {
    fakeApi({
      "POST /api/v1/accounts": () => apiError(422, "validation_failed", "name is too long"),
    });
    renderSelect();
    await userEvent.type(field(), "Gym");
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByText(/name is too long/)).toBeInTheDocument();
    expect(value()).toBe("");
  });
});
