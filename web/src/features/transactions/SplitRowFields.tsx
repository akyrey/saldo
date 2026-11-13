import { Trash2 } from "lucide-react";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import type { Account, Currency } from "@/features/accounts/queries";
import type { Category } from "@/features/categories/queries";
import type { Tag } from "@/features/tags/queries";
import { cn } from "@/lib/utils";
import { AccountSelect } from "./AccountSelect";
import { deriveKind, KIND_LABELS, type SplitRow } from "./editor-model";
import { TagPicker } from "./TagPicker";

export interface SplitRowFieldsProps {
  index: number;
  row: SplitRow;
  errors: Record<string, string> | undefined;
  accounts: readonly Account[];
  categories: readonly Category[];
  tags: readonly Tag[];
  currencies: readonly Currency[];
  canRemove: boolean;
  onChange: (changes: Partial<Omit<SplitRow, "key">>) => void;
  onRemove: () => void;
  /** A recurrence template: no date (each firing sets it) and no notes. */
  template?: boolean;
}

/** One split: date, description, accounts, amounts, category and tags. */
export function SplitRowFields(props: SplitRowFieldsProps) {
  const { index, row, errors = {}, accounts, onChange } = props;
  const id = (field: string) => `split-${index}-${field}`;
  const source = accounts.find((a) => a.id === row.source_id);
  const destination = accounts.find((a) => a.id === row.destination_id);
  const kind = deriveKind(source, destination);
  const currency = [source, destination].find(
    (a) => a && a.type !== "expense" && a.type !== "revenue" && a.type !== "equity",
  )?.currency;
  const showForeign =
    row.foreign_amount !== "" ||
    row.foreign_currency !== "" ||
    Boolean(errors.foreign_amount ?? errors.foreign_currency);

  return (
    <fieldset
      aria-label={`Split ${index + 1}`}
      className={cn("rounded-lg border p-3 sm:p-4", errors.row && "border-destructive")}
    >
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="text-sm font-medium">Split {index + 1}</h2>
        <div className="flex items-center gap-2">
          {kind ? (
            <Badge
              data-testid={id("kind")}
              className={cn(kind === "invalid" && "border-destructive text-destructive")}
            >
              {KIND_LABELS[kind]}
            </Badge>
          ) : null}
          {props.canRemove ? (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={`Remove split ${index + 1}`}
              onClick={props.onRemove}
            >
              <Trash2 />
            </Button>
          ) : null}
        </div>
      </div>
      {errors.row ? (
        <p className="mb-3 text-sm text-destructive" data-testid={id("row-error")}>
          {errors.row}
        </p>
      ) : null}
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {props.template ? null : (
          <Field id={id("date")} label="Date" error={errors.date}>
            <Input
              id={id("date")}
              type="date"
              required
              value={row.date}
              onChange={(e) => onChange({ date: e.target.value })}
              {...errorProps(id("date"), errors.date)}
            />
          </Field>
        )}
        <Field
          id={id("description")}
          label="Description"
          error={errors.description}
          className={props.template ? "sm:col-span-2 lg:col-span-4" : "lg:col-span-3"}
        >
          <Input
            id={id("description")}
            required
            value={row.description}
            onChange={(e) => onChange({ description: e.target.value })}
            {...errorProps(id("description"), errors.description)}
          />
        </Field>
        <Field
          id={id("source")}
          label="Source account"
          error={errors.source_id}
          className="lg:col-span-2"
        >
          <AccountSelect
            id={id("source")}
            required
            accounts={accounts}
            value={row.source_id}
            onChange={(e) => onChange({ source_id: e.target.value })}
            {...errorProps(id("source"), errors.source_id)}
          />
        </Field>
        <Field
          id={id("destination")}
          label="Destination account"
          error={errors.destination_id}
          className="lg:col-span-2"
        >
          <AccountSelect
            id={id("destination")}
            required
            accounts={accounts}
            value={row.destination_id}
            onChange={(e) => onChange({ destination_id: e.target.value })}
            {...errorProps(id("destination"), errors.destination_id)}
          />
        </Field>
        <Field
          id={id("amount")}
          label={currency ? `Amount (${currency})` : "Amount"}
          error={errors.amount ?? errors.currency}
        >
          <Input
            id={id("amount")}
            inputMode="decimal"
            autoComplete="off"
            required
            placeholder="0.00"
            value={row.amount}
            onChange={(e) => onChange({ amount: e.target.value })}
            {...errorProps(id("amount"), errors.amount ?? errors.currency)}
          />
        </Field>
        <Field id={id("category")} label="Category" error={errors.category_id}>
          <NativeSelect
            id={id("category")}
            value={row.category_id}
            onChange={(e) => onChange({ category_id: e.target.value })}
          >
            <option value="">None</option>
            {props.categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field id={id("tags")} label="Tags" error={errors.tag_ids} className="sm:col-span-2">
          <TagPicker
            id={id("tags")}
            tags={props.tags}
            value={row.tag_ids}
            onChange={(tag_ids) => onChange({ tag_ids })}
          />
        </Field>
      </div>
      <details className="mt-3" open={showForeign}>
        <summary className="cursor-pointer text-sm text-muted-foreground">
          {props.template ? "Foreign amount" : "Foreign amount and notes"}
        </summary>
        <div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Field id={id("foreign_amount")} label="Foreign amount" error={errors.foreign_amount}>
            <Input
              id={id("foreign_amount")}
              inputMode="decimal"
              autoComplete="off"
              value={row.foreign_amount}
              onChange={(e) => onChange({ foreign_amount: e.target.value })}
              {...errorProps(id("foreign_amount"), errors.foreign_amount)}
            />
          </Field>
          <Field
            id={id("foreign_currency")}
            label="Foreign currency"
            error={errors.foreign_currency}
          >
            <NativeSelect
              id={id("foreign_currency")}
              value={row.foreign_currency}
              onChange={(e) => onChange({ foreign_currency: e.target.value })}
              {...errorProps(id("foreign_currency"), errors.foreign_currency)}
            >
              <option value="">None</option>
              {props.currencies.map((c) => (
                <option key={c.code} value={c.code}>
                  {c.code}
                </option>
              ))}
            </NativeSelect>
          </Field>
          {props.template ? null : (
            <Field id={id("notes")} label="Notes" error={errors.notes} className="sm:col-span-2">
              <Input
                id={id("notes")}
                value={row.notes}
                onChange={(e) => onChange({ notes: e.target.value })}
              />
            </Field>
          )}
        </div>
      </details>
    </fieldset>
  );
}
