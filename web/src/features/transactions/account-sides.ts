// Which account types make sense on each side of a split, so the editor's
// pickers offer only accounts the server would accept (see `deriveKind`).
import type { AccountKindInfo, TemplateKind } from "./editor-model";
import { accountTypesFor } from "./prefill";

export type Side = "source" | "destination";

/** Money leaves own accounts, revenue and equity; it reaches own accounts, expenses and equity. */
const ANY_PARTNER: Record<Side, readonly string[]> = {
  source: ["asset", "liability", "revenue", "equity"],
  destination: ["asset", "liability", "expense", "equity"],
};

/** Expense, revenue and equity accounts only ever pair with an own account. */
const OWN_ACCOUNTS: readonly string[] = ["asset", "liability"];

/**
 * The account types to offer on `side`, given the account already chosen on
 * the other side and, from a template, the transaction's kind.
 */
export function sideTypes(
  side: Side,
  other: AccountKindInfo | undefined,
  kind?: TemplateKind,
): readonly string[] {
  const otherIsOwn = other === undefined || OWN_ACCOUNTS.includes(other.type);
  const partners = otherIsOwn ? ANY_PARTNER[side] : OWN_ACCOUNTS;
  const forKind = accountTypesFor(kind)?.[side];
  return forKind ? partners.filter((type) => forKind.includes(type)) : partners;
}
