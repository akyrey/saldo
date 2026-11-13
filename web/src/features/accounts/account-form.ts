import type { components } from "@/api/schema";
import type { Account } from "./queries";

type CreateAccountRequest = components["schemas"]["CreateAccountRequest"];
type AccountPatch = components["schemas"]["AccountPatch"];

export const CREATABLE_TYPES = ["asset", "liability", "expense", "revenue"] as const;
export const ASSET_ROLES = ["default", "shared", "savings", "credit_card", "cash_wallet"] as const;
export const LIABILITY_TYPES = ["loan", "debt", "mortgage"] as const;
export const LIABILITY_DIRECTIONS = ["i_owe", "owed_to_me"] as const;

/** The account form's values; every input is a string or a boolean. */
export interface AccountForm {
  name: string;
  type: string;
  currency: string;
  owner_user_id: string;
  asset_role: string;
  liability_type: string;
  liability_direction: string;
  iban: string;
  account_number: string;
  include_net_worth: boolean;
  notes: string;
}

export function emptyAccountForm(currency: string): AccountForm {
  return {
    name: "",
    type: "asset",
    currency,
    owner_user_id: "",
    asset_role: "",
    liability_type: "",
    liability_direction: "",
    iban: "",
    account_number: "",
    include_net_worth: true,
    notes: "",
  };
}

export function fromAccount(account: Account): AccountForm {
  return {
    name: account.name,
    type: account.type,
    currency: account.currency,
    owner_user_id: account.owner_user_id ?? "",
    asset_role: account.asset_role ?? "",
    liability_type: account.liability_type ?? "",
    liability_direction: account.liability_direction ?? "",
    iban: account.iban ?? "",
    account_number: account.account_number ?? "",
    include_net_worth: account.include_net_worth,
    notes: account.notes ?? "",
  };
}

const orNull = (value: string) => (value.trim() === "" ? null : value.trim());

/** Fields that only some types allow are sent as null on the others. */
function typedFields(form: AccountForm) {
  const isAsset = form.type === "asset";
  const isLiability = form.type === "liability";
  return {
    owner_user_id: orNull(form.owner_user_id),
    asset_role: isAsset ? orNull(form.asset_role) : null,
    liability_type: isLiability ? orNull(form.liability_type) : null,
    liability_direction: isLiability ? orNull(form.liability_direction) : null,
    iban: orNull(form.iban),
    account_number: orNull(form.account_number),
    include_net_worth: form.include_net_worth,
    notes: orNull(form.notes),
  };
}

export function toCreateRequest(form: AccountForm): CreateAccountRequest {
  return { name: form.name.trim(), type: form.type, currency: form.currency, ...typedFields(form) };
}

/** Type and currency never change, so the patch leaves them out. */
export function toPatch(form: AccountForm): AccountPatch {
  return { name: form.name.trim(), ...typedFields(form) };
}
