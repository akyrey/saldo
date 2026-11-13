import { useQuery } from "@tanstack/react-query";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { accountsQuery, currenciesQuery } from "@/features/accounts/queries";
import { billsQuery } from "@/features/bills/queries";
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

/** The right control for a trigger's or action's value: text, or a picker. */
export function ValueInput({ id, label, kind, value, onChange }: ValueInputProps) {
  const type = valueKind(kind);
  const categories = useQuery({ ...categoriesQuery, enabled: type === "category" });
  const tags = useQuery({ ...tagsQuery, enabled: type === "tag" });
  const accounts = useQuery({ ...accountsQuery(), enabled: type === "account" });
  const bills = useQuery({ ...billsQuery, enabled: type === "bill" });
  const currencies = useQuery({ ...currenciesQuery, enabled: type === "currency" });

  const options: { value: string; label: string }[] | null =
    type === "category"
      ? (categories.data ?? []).map((c) => ({ value: c.id, label: c.name }))
      : type === "tag"
        ? (tags.data ?? []).map((t) => ({ value: t.id, label: t.name }))
        : type === "account"
          ? (accounts.data ?? []).map((a) => ({ value: a.id, label: `${a.name} (${a.type})` }))
          : type === "bill"
            ? (bills.data ?? []).map((b) => ({ value: b.id, label: b.name }))
            : type === "currency"
              ? (currencies.data ?? []).map((c) => ({
                  value: c.code,
                  label: `${c.code} · ${c.name}`,
                }))
              : type === "kind"
                ? SPLIT_KINDS.map((k) => ({ value: k, label: k.replace("_", " ") }))
                : null;

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
      placeholder={type === "budget" ? "Budget ID" : type === "amount" ? "e.g. 12.50" : ""}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}
