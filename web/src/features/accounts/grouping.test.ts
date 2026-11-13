import { describe, expect, it } from "vitest";
import { groupAccounts, ownerLabel } from "./grouping";
import type { Account, Balance } from "./queries";

function account(id: string, type: string, currency = "EUR"): Account {
  return {
    id,
    name: id,
    type,
    currency,
    virtual_balance: "0.00",
    include_net_worth: true,
    active: true,
    sort_order: 0,
    created_at: "",
    updated_at: "",
  };
}

function balance(id: string, amount: string, currency = "EUR"): [string, Balance] {
  return [id, { account_id: id, balance: amount, currency }];
}

describe("groupAccounts", () => {
  it("orders groups by type and totals balances per currency exactly", () => {
    const accounts = [
      account("food", "expense"),
      account("a", "asset"),
      account("b", "asset"),
      account("c", "asset", "JPY"),
    ];
    const balances = new Map([
      balance("a", "0.10"),
      balance("b", "0.20"),
      balance("c", "1500", "JPY"),
      balance("food", "9.99"),
    ]);
    const currencies = [
      { code: "EUR", name: "Euro", symbol: "€", decimal_places: 2 },
      { code: "JPY", name: "Yen", symbol: "¥", decimal_places: 0 },
    ];

    const groups = groupAccounts(accounts, balances, currencies);

    expect(groups.map((g) => g.type)).toEqual(["asset", "expense"]);
    expect(groups[0]?.totals).toEqual([
      { currency: "EUR", amount: "0.30" },
      { currency: "JPY", amount: "1500" },
    ]);
    expect(groups[1]?.totals).toEqual([{ currency: "EUR", amount: "9.99" }]);
  });

  it("skips accounts without a balance yet", () => {
    const groups = groupAccounts([account("a", "asset")], new Map(), undefined);
    expect(groups[0]?.totals).toEqual([]);
  });
});

describe("ownerLabel", () => {
  it("names the owner or says joint", () => {
    const members = [{ user_id: "u1", display_name: "Ada" }];
    expect(ownerLabel(null, members)).toBe("Joint");
    expect(ownerLabel("u1", members)).toBe("Ada");
    expect(ownerLabel("u2", members)).toBe("Member");
  });
});
