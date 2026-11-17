import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { useState } from "react";
import { ApiError, errorMessage } from "@/api/errors";
import { DatePicker } from "@/components/DatePicker";
import { Field } from "@/components/Field";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Combobox } from "@/components/Combobox";
import { accountsQuery, currenciesQuery } from "@/features/accounts/queries";
import { billsQuery } from "@/features/bills/queries";
import { budgetsQuery } from "@/features/budgets/queries";
import { categoriesQuery } from "@/features/categories/queries";
import { piggyBanksQuery } from "@/features/piggy-banks/queries";
import { tagsQuery } from "@/features/tags/queries";
import { mapServerErrors, type EditorErrors } from "@/features/transactions/editor-model";
import { SplitRowFields } from "@/features/transactions/SplitRowFields";
import type { RecurrenceRequest } from "./queries";
import {
  emptyRepetition,
  emptyTemplate,
  toPreviewRequest,
  toRequest,
  updateRepetition,
  type RecurrenceState,
  type TemplateRow,
} from "./recurrence-model";
import { RepetitionFields } from "./RepetitionFields";
import { SchedulePreview } from "./SchedulePreview";

const NO_ERRORS: EditorErrors = { rows: {}, form: [] };

interface RecurrenceEditorProps {
  initial: RecurrenceState;
  submitLabel: string;
  save: (body: RecurrenceRequest) => Promise<unknown>;
  onSaved: () => void;
}

/** Schedule (first date, repetitions, limits) plus the template splits. */
export function RecurrenceEditor({ initial, submitLabel, save, onSaved }: RecurrenceEditorProps) {
  const queryClient = useQueryClient();
  const [state, setState] = useState(initial);
  const [errors, setErrors] = useState<EditorErrors>(NO_ERRORS);
  const [accounts, categories, tags, currencies, piggyBanks, budgets, bills] = useQueries({
    queries: [
      accountsQuery(),
      categoriesQuery,
      tagsQuery,
      currenciesQuery,
      piggyBanksQuery,
      budgetsQuery,
      billsQuery,
    ],
  });
  const mutation = useMutation({
    mutationFn: () => save(toRequest(state)),
    onMutate: () => setErrors(NO_ERRORS),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["recurrences"] });
      onSaved();
    },
    onError: (error) =>
      setErrors(
        error instanceof ApiError && error.details.length > 0
          ? mapServerErrors(error.details)
          : { rows: {}, form: [errorMessage(error)] },
      ),
  });
  const set = (changes: Partial<RecurrenceState>) => setState((s) => ({ ...s, ...changes }));
  const setSplit = (index: number, changes: Partial<TemplateRow>) =>
    setState((s) => ({
      ...s,
      splits: s.splits.map((row, i) => (i === index ? { ...row, ...changes } : row)),
    }));

  const loadError = accounts.error ?? categories.error ?? tags.error ?? currencies.error;
  if (!accounts.data || loadError) {
    return <QueryState isPending={accounts.isPending} error={loadError} />;
  }

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate();
      }}
    >
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Field id="title" label="Title" className="sm:col-span-2">
          <Input
            id="title"
            required
            value={state.title}
            onChange={(e) => set({ title: e.target.value })}
          />
        </Field>
        <Field id="first-date" label="First date">
          <DatePicker
            id="first-date"
            required
            value={state.first_date}
            onChange={(value) => set({ first_date: value })}
          />
        </Field>
        <Field id="repeat-until" label="Repeat until (optional)">
          <DatePicker
            id="repeat-until"
            value={state.repeat_until}
            onChange={(value) => set({ repeat_until: value })}
          />
        </Field>
        <Field id="max-occurrences" label="Stop after (times, optional)">
          <Input
            id="max-occurrences"
            type="number"
            min={1}
            value={state.max_occurrences}
            onChange={(e) => set({ max_occurrences: e.target.value })}
          />
        </Field>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={state.active}
            onChange={(e) => set({ active: e.target.checked })}
          />
          Active
        </label>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={state.apply_rules}
            onChange={(e) => set({ apply_rules: e.target.checked })}
          />
          Apply rules
        </label>
      </div>
      {state.repetitions.map((row, index) => (
        <RepetitionFields
          key={row.key}
          index={index}
          row={row}
          canRemove={state.repetitions.length > 1}
          onChange={(changes) => setState((s) => updateRepetition(s, index, changes))}
          onRemove={() =>
            setState((s) => ({ ...s, repetitions: s.repetitions.filter((_, i) => i !== index) }))
          }
        />
      ))}
      <div>
        <Button
          type="button"
          variant="outline"
          onClick={() =>
            setState((s) => ({
              ...s,
              repetitions: [...s.repetitions, emptyRepetition(s.first_date)],
            }))
          }
        >
          <Plus /> Add repetition
        </Button>
      </div>
      <SchedulePreview request={toPreviewRequest(state)} />
      {state.splits.map((row, index) => (
        <div key={row.key} className="flex flex-col gap-2">
          <SplitRowFields
            template
            index={index}
            row={row}
            errors={errors.rows[index]}
            accounts={accounts.data}
            categories={categories.data ?? []}
            tags={tags.data ?? []}
            currencies={currencies.data ?? []}
            budgets={budgets.data ?? []}
            bills={bills.data ?? []}
            canRemove={state.splits.length > 1}
            onChange={(changes) => setSplit(index, changes)}
            onRemove={() =>
              setState((s) => ({ ...s, splits: s.splits.filter((_, i) => i !== index) }))
            }
          />
          {piggyBanks.data && piggyBanks.data.length > 0 ? (
            <Field
              id={`split-${index}-piggy`}
              label={`Piggy bank for split ${index + 1} (transfers only)`}
            >
              <Combobox
                id={`split-${index}-piggy`}
                label="Piggy bank"
                clearable
                placeholder="None"
                value={row.piggy_bank_id}
                onChange={(piggy_bank_id) => setSplit(index, { piggy_bank_id })}
                options={piggyBanks.data.map((p) => ({ value: p.id, label: p.name }))}
              />
            </Field>
          ) : null}
        </div>
      ))}
      <div>
        <Button
          type="button"
          variant="outline"
          onClick={() => setState((s) => ({ ...s, splits: [...s.splits, emptyTemplate()] }))}
        >
          <Plus /> Add split
        </Button>
      </div>
      {errors.form.length > 0 ? (
        <Alert>
          {errors.form.map((message) => (
            <p key={message}>{message}</p>
          ))}
        </Alert>
      ) : null}
      <div>
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending ? "Saving…" : submitLabel}
        </Button>
      </div>
    </form>
  );
}
