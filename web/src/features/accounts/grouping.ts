import { fromMinor, toMinor } from "@/api/money";
import type { Account, AccountPage, Balance, Currency } from "./queries";
import { placesOf } from "./queries";

export const ACCOUNT_TYPES = ["asset", "liability", "revenue", "expense", "equity"] as const;
export type AccountType = (typeof ACCOUNT_TYPES)[number];

export const TYPE_LABELS: Record<string, string> = {
  asset: "Asset accounts",
  liability: "Liabilities",
  revenue: "Revenue accounts",
  expense: "Expense accounts",
  equity: "Equity",
};

export interface AccountGroup {
  type: string;
  accounts: Account[];
  /** One total per currency, as decimal strings, sorted by currency code. */
  totals: { currency: string; amount: string }[];
}

/**
 * Groups accounts by type in a fixed order and adds up their balances per
 * currency, exactly (bigint minor units, never floats).
 */
export function groupAccounts(
  accounts: readonly Account[],
  balances: ReadonlyMap<string, Balance>,
  currencies: readonly Currency[] | undefined,
): AccountGroup[] {
  return ACCOUNT_TYPES.map((type) => {
    const members = accounts.filter((account) => account.type === type);
    const sums = new Map<string, bigint>();
    for (const account of members) {
      const balance = balances.get(account.id);
      if (!balance) continue;
      const places = placesOf(currencies, balance.currency);
      const minor = toMinor(balance.balance, places) ?? 0n;
      sums.set(balance.currency, (sums.get(balance.currency) ?? 0n) + minor);
    }
    const totals = [...sums.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([currency, minor]) => ({
        currency,
        amount: fromMinor(minor, placesOf(currencies, currency)),
      }));
    return { type, accounts: members, totals };
  }).filter((group) => group.accounts.length > 0);
}

/** The accounts of the pages loaded so far, their balances by ID, and the full count. */
export function flattenPages(pages: readonly AccountPage[] | undefined): {
  accounts: Account[];
  balances: Map<string, Balance>;
  total: number;
} {
  const loaded = pages ?? [];
  return {
    accounts: loaded.flatMap((page) => page.accounts),
    balances: new Map(
      loaded.flatMap((page) => page.balances).map((balance) => [balance.account_id, balance]),
    ),
    total: loaded.at(-1)?.meta.total ?? 0,
  };
}

/** "Joint" or the owner's name. */
export function ownerLabel(
  ownerId: string | null | undefined,
  members: readonly { user_id: string; display_name: string }[] | undefined,
): string {
  if (!ownerId) return "Joint";
  return members?.find((m) => m.user_id === ownerId)?.display_name ?? "Member";
}
