import { describe, expect, it } from "vitest";
import { emptyAccountForm, fromAccount, toCreateRequest, toPatch } from "./account-form";
import type { Account } from "./queries";

describe("account form", () => {
  it("builds a create request with nulls for empty and type-foreign fields", () => {
    const form = {
      ...emptyAccountForm("EUR"),
      name: " Checking ",
      asset_role: "default",
      liability_type: "loan",
    };
    expect(toCreateRequest(form)).toEqual({
      name: "Checking",
      type: "asset",
      currency: "EUR",
      owner_user_id: null,
      asset_role: "default",
      liability_type: null,
      liability_direction: null,
      iban: null,
      account_number: null,
      include_net_worth: true,
      notes: null,
    });
  });

  it("round-trips an account into a patch without type or currency", () => {
    const account: Account = {
      id: "a",
      name: "Mortgage",
      type: "liability",
      currency: "EUR",
      owner_user_id: "u1",
      liability_type: "mortgage",
      liability_direction: "i_owe",
      iban: "DE00",
      virtual_balance: "0.00",
      include_net_worth: false,
      active: true,
      sort_order: 0,
      created_at: "",
      updated_at: "",
    };
    const patch = toPatch(fromAccount(account));
    expect(patch).toEqual({
      name: "Mortgage",
      owner_user_id: "u1",
      asset_role: null,
      liability_type: "mortgage",
      liability_direction: "i_owe",
      iban: "DE00",
      account_number: null,
      include_net_worth: false,
      notes: null,
    });
    expect(patch).not.toHaveProperty("type");
  });
});
