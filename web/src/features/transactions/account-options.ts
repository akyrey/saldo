import type { ComboboxOption } from "@/components/Combobox";
import { ACCOUNT_TYPES, TYPE_LABELS } from "@/features/accounts/grouping";
import type { Account } from "@/features/accounts/queries";

/** Accounts in type order, as `Name (EUR)` with the type as a searchable hint. */
export function accountOptions(
  accounts: readonly Account[],
  value: string,
  types: readonly string[] = ACCOUNT_TYPES,
): ComboboxOption[] {
  return ACCOUNT_TYPES.flatMap((type) =>
    accounts
      // Archived accounts only when already chosen (old splits may use them).
      .filter((a) => a.type === type && ((a.active && types.includes(type)) || a.id === value))
      .map((a) => ({
        value: a.id,
        label: `${a.name} (${a.currency})`,
        hint: TYPE_LABELS[type],
      })),
  );
}
