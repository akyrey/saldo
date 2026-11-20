import { ChevronDown, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import { Combobox } from "@/components/Combobox";
import { DatePicker } from "@/components/DatePicker";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import type { Account } from "@/features/accounts/queries";
import type { Bill } from "@/features/bills/queries";
import type { Budget } from "@/features/budgets/queries";
import type { Category } from "@/features/categories/queries";
import type { Tag } from "@/features/tags/queries";
import { cn } from "@/lib/utils";
import { AccountSelect } from "./AccountSelect";
import { KIND_LABELS } from "./editor-model";
import { AmountFilter } from "./AmountFilter";
import { activeFilterCount, NONE, SPLIT_KINDS, type TransactionSearch } from "./search";

interface TransactionFiltersProps {
  value: TransactionSearch;
  accounts: readonly Account[];
  categories: readonly Category[];
  tags: readonly Tag[];
  budgets: readonly Budget[];
  bills: readonly Bill[];
  /** Called with the changed filters; the page resets to 1. */
  onChange: (changes: Partial<TransactionSearch>) => void;
}

const orUndefined = (value: string) => (value === "" ? undefined : value);

/** `"true"`/`"false"` from a select, or undefined for "any". */
function flagOf(value: string): boolean | undefined {
  return value === "" ? undefined : value === "true";
}

function flagText(value: boolean | undefined): string {
  return value === undefined ? "" : String(value);
}

/** Filters that live in the URL, so a filtered list can be bookmarked. */
export function TransactionFilters({
  value,
  accounts,
  categories,
  tags,
  budgets,
  bills,
  onChange,
}: TransactionFiltersProps) {
  const [text, setText] = useState(value.q ?? "");
  const [open, setOpen] = useState(false);
  const active = activeFilterCount(value);
  return (
    <div className="mb-4 rounded-lg border p-3">
      <div className="flex items-end gap-2">
        <form
          className="flex flex-1 items-end gap-2"
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
        <Button
          type="button"
          variant="outline"
          aria-expanded={open}
          aria-controls="transaction-filters"
          onClick={() => setOpen(!open)}
        >
          <SlidersHorizontal />
          <span className="sr-only sm:not-sr-only">Filters</span>
          {active > 0 ? (
            <span className="rounded-full bg-primary px-1.5 text-xs text-primary-foreground">
              {active}
              <span className="sr-only"> active</span>
            </span>
          ) : null}
          <ChevronDown className={cn("transition-transform", open && "rotate-180")} />
        </Button>
      </div>
      <div
        id="transaction-filters"
        hidden={!open}
        className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3"
      >
        <Field id="filter-from" label="From">
          <DatePicker
            id="filter-from"
            value={value.from ?? ""}
            onChange={(value) => onChange({ from: orUndefined(value) })}
          />
        </Field>
        <Field id="filter-to" label="To">
          <DatePicker
            id="filter-to"
            value={value.to ?? ""}
            onChange={(value) => onChange({ to: orUndefined(value) })}
          />
        </Field>
        <Field id="filter-account" label="Account">
          <AccountSelect
            id="filter-account"
            accounts={accounts}
            label="Account"
            placeholder="Any account"
            clearable
            value={value.account_id ?? ""}
            onChange={(accountId) => onChange({ account_id: orUndefined(accountId) })}
          />
        </Field>
        <Field id="filter-category" label="Category">
          <Combobox
            id="filter-category"
            label="Category"
            clearable
            placeholder="Any category"
            value={value.category_id ?? ""}
            onChange={(categoryId) => onChange({ category_id: orUndefined(categoryId) })}
            options={[
              { value: NONE, label: "No category" },
              ...categories.map((c) => ({ value: c.id, label: c.name })),
            ]}
          />
        </Field>
        <Field id="filter-tag" label="Tag">
          <Combobox
            id="filter-tag"
            label="Tag"
            clearable
            placeholder="Any tag"
            value={value.tag_id ?? ""}
            onChange={(tagId) => onChange({ tag_id: orUndefined(tagId) })}
            options={tags.map((t) => ({ value: t.id, label: t.name }))}
          />
        </Field>
        <Field id="filter-budget" label="Budget">
          <Combobox
            id="filter-budget"
            label="Budget"
            clearable
            placeholder="Any budget"
            value={value.budget_id ?? ""}
            onChange={(budgetId) => onChange({ budget_id: orUndefined(budgetId) })}
            options={[
              { value: NONE, label: "No budget" },
              ...budgets.map((b) => ({ value: b.id, label: b.name })),
            ]}
          />
        </Field>
        <Field id="filter-bill" label="Bill">
          <Combobox
            id="filter-bill"
            label="Bill"
            clearable
            placeholder="Any bill"
            value={value.bill_id ?? ""}
            onChange={(billId) => onChange({ bill_id: orUndefined(billId) })}
            options={bills.map((b) => ({ value: b.id, label: b.name }))}
          />
        </Field>
        <Field id="filter-kind" label="Kind">
          <NativeSelect
            id="filter-kind"
            value={value.kind ?? ""}
            onChange={(e) =>
              onChange({ kind: SPLIT_KINDS.find((kind) => kind === e.target.value) })
            }
          >
            <option value="">Any kind</option>
            {SPLIT_KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {KIND_LABELS[kind]}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <AmountFilter
          id="filter-amount-min"
          label="Amount from"
          value={value.amount_min}
          onChange={(amount_min) => onChange({ amount_min })}
        />
        <AmountFilter
          id="filter-amount-max"
          label="Amount up to"
          value={value.amount_max}
          onChange={(amount_max) => onChange({ amount_max })}
        />
        <Field id="filter-reconciled" label="Reconciled">
          <NativeSelect
            id="filter-reconciled"
            value={flagText(value.reconciled)}
            onChange={(e) => onChange({ reconciled: flagOf(e.target.value) })}
          >
            <option value="">Any</option>
            <option value="true">Reconciled</option>
            <option value="false">Not reconciled</option>
          </NativeSelect>
        </Field>
        <Field id="filter-attachments" label="Attachments">
          <NativeSelect
            id="filter-attachments"
            value={flagText(value.has_attachments)}
            onChange={(e) => onChange({ has_attachments: flagOf(e.target.value) })}
          >
            <option value="">Any</option>
            <option value="true">With attachments</option>
            <option value="false">Without attachments</option>
          </NativeSelect>
        </Field>
      </div>
    </div>
  );
}
