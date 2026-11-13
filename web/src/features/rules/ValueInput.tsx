import { useQuery } from "@tanstack/react-query";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { accountsQuery, currenciesQuery } from "@/features/accounts/queries";
import { billsQuery } from "@/features/bills/queries";
import { budgetsQuery } from "@/features/budgets/queries";
import { categoriesQuery } from "@/features/categories/queries";
import { tagsQuery } from "@/features/tags/queries";
import { SPLIT_KINDS, valueKind } from "./rule-form";

interface ValueInputProps {
  id: string;
  label: string;
  kind: string;
  value: string;
  onChange: (value: string) => void;
}

interface Option {
  value: string;
  label: string;
}

/** The right control for a trigger's or action's value: text, or a picker. */
export function ValueInput({ id, label, kind, value, onChange }: ValueInputProps) {
  const type = valueKind(kind);
  const categories = useQuery({ ...categoriesQuery, enabled: type === "category" });
  const tags = useQuery({ ...tagsQuery, enabled: type === "tag" });
  const accounts = useQuery({ ...accountsQuery(), enabled: type === "account" });
  const bills = useQuery({ ...billsQuery, enabled: type === "bill" });
  const budgets = useQuery({ ...budgetsQuery, enabled: type === "budget" });
  const currencies = useQuery({ ...currenciesQuery, enabled: type === "currency" });

  // The picker's options for each value kind; kinds missing here are typed.
  const pickers: Record<string, () => Option[]> = {
    category: () => (categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
    tag: () => (tags.data ?? []).map((t) => ({ value: t.id, label: t.name })),
    account: () =>
      (accounts.data ?? []).map((a) => ({ value: a.id, label: `${a.name} (${a.type})` })),
    bill: () => (bills.data ?? []).map((b) => ({ value: b.id, label: b.name })),
    budget: () => (budgets.data ?? []).map((b) => ({ value: b.id, label: b.name })),
    currency: () =>
      (currencies.data ?? []).map((c) => ({ value: c.code, label: `${c.code} · ${c.name}` })),
    kind: () => SPLIT_KINDS.map((k) => ({ value: k, label: k.replace("_", " ") })),
  };
  const options = pickers[type]?.() ?? null;

  if (type === "none") return null;
  if (options) {
    return (
      <NativeSelect
        id={id}
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">Choose…</option>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </NativeSelect>
    );
  }
  return (
    <Input
      id={id}
      aria-label={label}
      inputMode={type === "amount" ? "decimal" : undefined}
      placeholder={type === "amount" ? "e.g. 12.50" : ""}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}
