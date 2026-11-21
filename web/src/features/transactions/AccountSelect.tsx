import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Combobox, type ComboboxCreate } from "@/components/Combobox";
import { accountsQuery, type Account } from "@/features/accounts/queries";
import { accountOptions } from "./account-options";

/** Lets the picker create an expense or revenue account from the typed name. */
export interface AccountCreate {
  type: "expense" | "revenue";
  currency: string;
}

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
  /** Offer "Create … account" for a name no account of that type has. */
  create?: AccountCreate;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
}

const sameName = (a: string, b: string) => a.toLocaleLowerCase() === b.toLocaleLowerCase();

/** Type an account's name (or its type) to find it, or to create a new one. */
export function AccountSelect({
  accounts,
  value,
  types,
  placeholder,
  create,
  onChange,
  ...props
}: AccountSelectProps) {
  const queryClient = useQueryClient();
  const creation = useMutation({
    mutationFn: async ({ name, type, currency }: AccountCreate & { name: string }) =>
      (await call(api.POST("/api/v1/accounts", { body: { name, type, currency } }))).data,
    onSuccess: (account) => {
      // Into the shared list at once, so every picker can show the new name.
      queryClient.setQueryData(accountsQuery().queryKey, (old: Account[] | undefined) => [
        ...(old ?? []),
        account,
      ]);
      onChange(account.id);
    },
  });

  const createRow: ComboboxCreate | undefined = create && {
    label: (name) =>
      accounts.some((a) => a.active && a.type === create.type && sameName(a.name, name))
        ? null
        : `Create ${create.type} account “${name}”`,
    onCreate: (name) => creation.mutate({ ...create, name }),
  };

  return (
    <div className="flex flex-col gap-1">
      <Combobox
        {...props}
        value={value}
        onChange={onChange}
        placeholder={placeholder ?? "Type an account…"}
        options={accountOptions(accounts, value, types)}
        create={createRow}
      />
      {creation.error ? (
        <p className="text-xs text-destructive">{errorMessage(creation.error)}</p>
      ) : null}
    </div>
  );
}
