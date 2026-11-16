import { Trash2 } from "lucide-react";
import { DatePicker } from "@/components/DatePicker";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Combobox } from "@/components/Combobox";
import { DescriptionInput } from "./DescriptionInput";
import type { Account, Currency } from "@/features/accounts/queries";
import type { Budget } from "@/features/budgets/queries";
import type { Category } from "@/features/categories/queries";
import type { Tag } from "@/features/tags/queries";
import { cn } from "@/lib/utils";
import { AccountSelect } from "./AccountSelect";
import { deriveKind, KIND_LABELS, type SplitRow, type TemplateKind } from "./editor-model";
import { sideTypes } from "./account-sides";
import { TagPicker } from "./TagPicker";

export interface SplitRowFieldsProps {
  index: number;
  row: SplitRow;
  errors: Record<string, string> | undefined;
  accounts: readonly Account[];
  categories: readonly Category[];
  budgets: readonly Budget[];
  tags: readonly Tag[];
  currencies: readonly Currency[];
  canRemove: boolean;
  onChange: (changes: Partial<Omit<SplitRow, "key">>) => void;
  onRemove: () => void;
  /** A recurrence template: no date (each firing sets it) and no notes. */
  template?: boolean;
  /** From a transaction template: offer only the accounts of this kind. */
  kindHint?: TemplateKind;
}

/** One split: date, description, accounts, amounts, category and tags. */
export function SplitRowFields(props: SplitRowFieldsProps) {
  const { index, row, errors = {}, accounts, onChange } = props;
  const id = (field: string) => `split-${index}-${field}`;
  const source = accounts.find((a) => a.id === row.source_id);
  const destination = accounts.find((a) => a.id === row.destination_id);
  const kind = deriveKind(source, destination);
  // Each picker offers only accounts that can pair with the other side's choice.
  const sourceChoices = accounts.filter((a) => a.id !== row.destination_id);
  const destinationChoices = accounts.filter((a) => a.id !== row.source_id);
  const budgetAllowed = kind === undefined || kind === "withdrawal";
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
            <DatePicker
              id={id("date")}
              required
              value={row.date}
              onChange={(value) => onChange({ date: value })}
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
          <DescriptionInput
            id={id("description")}
            required
            value={row.description}
            onChange={(description) => onChange({ description })}
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
            label="Source account"
            accounts={sourceChoices}
            types={sideTypes("source", destination, props.kindHint)}
            value={row.source_id}
            onChange={(source_id) => onChange({ source_id })}
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
            label="Destination account"
            accounts={destinationChoices}
            types={sideTypes("destination", source, props.kindHint)}
            value={row.destination_id}
            onChange={(destination_id) => onChange({ destination_id })}
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
          <Combobox
            id={id("category")}
            label="Category"
            clearable
            placeholder="None"
            value={row.category_id}
            onChange={(category_id) => onChange({ category_id })}
            options={props.categories.map((c) => ({ value: c.id, label: c.name }))}
          />
        </Field>
        <Field id={id("budget")} label="Budget" error={errors.budget_id}>
          <Combobox
            id={id("budget")}
            label="Budget"
            clearable
            placeholder={budgetAllowed ? "None" : "Withdrawals only"}
            value={row.budget_id}
            // Only withdrawals count against a budget; other kinds keep none.
            disabled={!budgetAllowed && row.budget_id === ""}
            onChange={(budget_id) => onChange({ budget_id })}
            options={props.budgets
              .filter((b) => b.active || b.id === row.budget_id)
              .map((b) => ({ value: b.id, label: b.name }))}
            {...errorProps(id("budget"), errors.budget_id)}
          />
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
            <Combobox
              id={id("foreign_currency")}
              label="Foreign currency"
              clearable
              placeholder="None"
              value={row.foreign_currency}
              onChange={(foreign_currency) => onChange({ foreign_currency })}
              options={props.currencies.map((c) => ({
                value: c.code,
                label: c.code,
                hint: c.name,
              }))}
              {...errorProps(id("foreign_currency"), errors.foreign_currency)}
            />
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
