import { Combobox } from "@/components/Combobox";
import type { Account } from "@/features/accounts/queries";
import { accountOptions } from "./account-options";

interface AccountSelectProps {
  id?: string;
  label?: string;
  accounts: readonly Account[];
  value: string;
  onChange: (accountId: string) => void;
  /** Shown while nothing is chosen. */
  placeholder?: string;
  /** Optional fields (filters) can be emptied again. */
  clearable?: boolean;
  /** Offer only these account types (the chosen account always shows). */
  types?: readonly string[];
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
}

/** Type an account's name (or its type) to find it. */
export function AccountSelect({
  accounts,
  value,
  types,
  placeholder,
  ...props
}: AccountSelectProps) {
  return (
    <Combobox
      {...props}
      value={value}
      placeholder={placeholder ?? "Type an account…"}
      options={accountOptions(accounts, value, types)}
    />
  );
}
