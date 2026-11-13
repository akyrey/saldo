import { useState } from "react";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import type { Account } from "@/features/accounts/queries";
import type { Category } from "@/features/categories/queries";
import type { Tag } from "@/features/tags/queries";
import { AccountSelect } from "./AccountSelect";
import { KIND_LABELS } from "./editor-model";
import { SPLIT_KINDS, type TransactionSearch } from "./search";

interface TransactionFiltersProps {
  value: TransactionSearch;
  accounts: readonly Account[];
  categories: readonly Category[];
  tags: readonly Tag[];
  /** Called with the changed filters; the page resets to 1. */
  onChange: (changes: Partial<TransactionSearch>) => void;
}

const orUndefined = (value: string) => (value === "" ? undefined : value);

/** Filters that live in the URL, so a filtered list can be bookmarked. */
export function TransactionFilters({
  value,
  accounts,
  categories,
  tags,
  onChange,
}: TransactionFiltersProps) {
  const [text, setText] = useState(value.q ?? "");
  return (
    <div className="mb-4 grid gap-3 rounded-lg border p-3 sm:grid-cols-2 lg:grid-cols-4">
      <form
        className="flex items-end gap-2 sm:col-span-2"
        onSubmit={(event) => {
          event.preventDefault();
          onChange({ q: orUndefined(text.trim()) });
        }}
      >
        <Field id="filter-q" label="Search" className="flex-1">
          <Input
            id="filter-q"
            type="search"
            placeholder="Description or title"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        </Field>
        <Button type="submit" variant="secondary">
          Search
        </Button>
      </form>
      <Field id="filter-from" label="From">
        <Input
          id="filter-from"
          type="date"
          value={value.from ?? ""}
          onChange={(e) => onChange({ from: orUndefined(e.target.value) })}
        />
      </Field>
      <Field id="filter-to" label="To">
        <Input
          id="filter-to"
          type="date"
          value={value.to ?? ""}
          onChange={(e) => onChange({ to: orUndefined(e.target.value) })}
        />
      </Field>
      <Field id="filter-account" label="Account">
        <AccountSelect
          id="filter-account"
          accounts={accounts}
          emptyLabel="Any account"
          value={value.account_id ?? ""}
          onChange={(e) => onChange({ account_id: orUndefined(e.target.value) })}
        />
      </Field>
      <Field id="filter-category" label="Category">
        <NativeSelect
          id="filter-category"
          value={value.category_id ?? ""}
          onChange={(e) => onChange({ category_id: orUndefined(e.target.value) })}
        >
          <option value="">Any category</option>
          {categories.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="filter-tag" label="Tag">
        <NativeSelect
          id="filter-tag"
          value={value.tag_id ?? ""}
          onChange={(e) => onChange({ tag_id: orUndefined(e.target.value) })}
        >
          <option value="">Any tag</option>
          {tags.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="filter-kind" label="Kind">
        <NativeSelect
          id="filter-kind"
          value={value.kind ?? ""}
          onChange={(e) => onChange({ kind: SPLIT_KINDS.find((kind) => kind === e.target.value) })}
        >
          <option value="">Any kind</option>
          {SPLIT_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {KIND_LABELS[kind]}
            </option>
          ))}
        </NativeSelect>
      </Field>
    </div>
  );
}
