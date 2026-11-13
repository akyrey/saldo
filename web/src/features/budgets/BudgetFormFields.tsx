import { Field } from "@/components/Field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { AUTO_TYPES, PERIODS, type BudgetForm } from "./budget-model";

interface BudgetFormFieldsProps {
  form: BudgetForm;
  errors: Record<string, string>;
  onChange: (changes: Partial<BudgetForm>) => void;
  /** Hides `active` and notes (the quick "add" form). */
  compact?: boolean;
}

/** Name, automatic limit and (unless compact) notes and the active switch. */
export function BudgetFormFields({ form, errors, onChange, compact }: BudgetFormFieldsProps) {
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
      <Field id="budget-name" label="Name" error={errors.name}>
        <Input
          id="budget-name"
          required
          value={form.name}
          onChange={(e) => onChange({ name: e.target.value })}
        />
      </Field>
      <Field id="budget-auto-type" label="Automatic limit" error={errors["auto_budget.type"]}>
        <NativeSelect
          id="budget-auto-type"
          value={form.autoType}
          onChange={(e) => onChange({ autoType: e.target.value })}
        >
          <option value="">None</option>
          {AUTO_TYPES.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </NativeSelect>
      </Field>
      {form.autoType !== "" ? (
        <>
          <Field id="budget-auto-amount" label="Amount" error={errors["auto_budget.amount"]}>
            <Input
              id="budget-auto-amount"
              inputMode="decimal"
              autoComplete="off"
              required
              placeholder="0.00"
              value={form.autoAmount}
              onChange={(e) => onChange({ autoAmount: e.target.value })}
            />
          </Field>
          <Field id="budget-auto-period" label="Per" error={errors["auto_budget.period"]}>
            <NativeSelect
              id="budget-auto-period"
              value={form.autoPeriod}
              onChange={(e) => onChange({ autoPeriod: e.target.value })}
            >
              {PERIODS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </NativeSelect>
          </Field>
        </>
      ) : null}
      {compact ? null : (
        <>
          <Field id="budget-notes" label="Notes" error={errors.notes} className="sm:col-span-2">
            <Input
              id="budget-notes"
              value={form.notes}
              onChange={(e) => onChange({ notes: e.target.value })}
            />
          </Field>
          <label className="flex items-center gap-2 self-end text-sm">
            <input
              type="checkbox"
              checked={form.active}
              onChange={(e) => onChange({ active: e.target.checked })}
            />
            Active
          </label>
        </>
      )}
    </div>
  );
}
