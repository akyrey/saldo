import type * as React from "react";
import { NativeSelect } from "@/components/ui/native-select";
import { ACCOUNT_TYPES, TYPE_LABELS } from "@/features/accounts/grouping";
import type { Account } from "@/features/accounts/queries";

type AccountSelectProps = Omit<React.ComponentProps<typeof NativeSelect>, "children"> & {
  accounts: readonly Account[];
  value: string;
  emptyLabel?: string;
  /** Offer only these account types (the chosen account always shows). */
  types?: readonly string[];
};

/**
 * Accounts grouped by type. Archived accounts appear only when already
 * chosen (old splits may use them; new ones may not).
 */
export function AccountSelect({
  accounts,
  value,
  emptyLabel = "Choose…",
  types = ACCOUNT_TYPES,
  ...props
}: AccountSelectProps) {
  return (
    <NativeSelect value={value} {...props}>
      <option value="">{emptyLabel}</option>
      {ACCOUNT_TYPES.map((type) => {
        const offered = (a: Account) => a.active && types.includes(type);
        const options = accounts.filter((a) => a.type === type && (offered(a) || a.id === value));
        if (options.length === 0) return null;
        return (
          <optgroup key={type} label={TYPE_LABELS[type]}>
            {options.map((account) => (
              <option key={account.id} value={account.id}>
                {account.name} ({account.currency})
              </option>
            ))}
          </optgroup>
        );
      })}
    </NativeSelect>
  );
}
