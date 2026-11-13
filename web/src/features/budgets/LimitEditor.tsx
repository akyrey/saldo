import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Trash2, X } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { normalizeAmountInput } from "@/api/money";
import { Field } from "@/components/Field";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { currenciesQuery, placesOf } from "@/features/accounts/queries";
import { todayIso } from "@/lib/dates";
import { monthOf, monthRange } from "./budget-model";
import { LimitProgress } from "./LimitProgress";
import { limitsQuery, type BudgetLimit } from "./queries";

interface LimitForm {
  start_date: string;
  end_date: string;
  amount: string;
}

function thisMonth(): LimitForm {
  const { from, to } = monthRange(monthOf(todayIso()));
  return { start_date: from, end_date: to, amount: "" };
}

interface LimitFormProps {
  initial: LimitForm;
  submitLabel: string;
  save: (form: LimitForm) => Promise<unknown>;
  onDone?: () => void;
  onCancel?: () => void;
}

/** Start, end and amount of a limit; used to add and to edit one. */
function LimitFormRow({ initial, submitLabel, save, onDone, onCancel }: LimitFormProps) {
  const [form, setForm] = useState(initial);
  const mutation = useMutation({
    mutationFn: () => save({ ...form, amount: normalizeAmountInput(form.amount) }),
    onSuccess: () => {
      setForm(initial);
      onDone?.();
    },
  });
  const errors = fieldErrors(mutation.error);
  const prefix = submitLabel === "Add limit" ? "new-limit" : "edit-limit";
  return (
    <form
      aria-label={submitLabel}
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate();
      }}
    >
      <div className="grid gap-3 sm:grid-cols-3">
        <Field id={`${prefix}-start`} label="From" error={errors.start_date}>
          <Input
            id={`${prefix}-start`}
            type="date"
            required
            value={form.start_date}
            onChange={(e) => setForm((f) => ({ ...f, start_date: e.target.value }))}
          />
        </Field>
        <Field id={`${prefix}-end`} label="To" error={errors.end_date}>
          <Input
            id={`${prefix}-end`}
            type="date"
            required
            value={form.end_date}
            onChange={(e) => setForm((f) => ({ ...f, end_date: e.target.value }))}
          />
        </Field>
        <Field id={`${prefix}-amount`} label="Limit" error={errors.amount}>
          <Input
            id={`${prefix}-amount`}
            inputMode="decimal"
            autoComplete="off"
            required
            placeholder="0.00"
            value={form.amount}
            onChange={(e) => setForm((f) => ({ ...f, amount: e.target.value }))}
          />
        </Field>
      </div>
      {mutation.error && Object.keys(errors).length === 0 ? (
        <Alert>{errorMessage(mutation.error)}</Alert>
      ) : null}
      <div className="flex gap-2">
        <Button type="submit" disabled={mutation.isPending}>
          {submitLabel}
        </Button>
        {onCancel ? (
          <Button type="button" variant="ghost" onClick={onCancel}>
            <X /> Cancel
          </Button>
        ) : null}
      </div>
    </form>
  );
}

function LimitRow({
  limit,
  places,
  onChanged,
}: {
  limit: BudgetLimit;
  places: number;
  onChanged: () => Promise<unknown>;
}) {
  const [editing, setEditing] = useState(false);
  const remove = useMutation({
    mutationFn: () =>
      call(api.DELETE("/api/v1/budget-limits/{id}", { params: { path: { id: limit.id } } })),
    onSuccess: onChanged,
  });
  if (editing) {
    return (
      <LimitFormRow
        initial={{ start_date: limit.start_date, end_date: limit.end_date, amount: limit.amount }}
        submitLabel="Save limit"
        save={(form) =>
          call(
            api.PATCH("/api/v1/budget-limits/{id}", {
              params: { path: { id: limit.id } },
              body: form,
            }),
          )
        }
        onDone={() => {
          setEditing(false);
          void onChanged();
        }}
        onCancel={() => setEditing(false)}
      />
    );
  }
  return (
    <div className="flex items-start gap-2">
      <div className="flex-1">
        <LimitProgress limit={limit} places={places} />
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label={`Edit limit ${limit.start_date}`}
        onClick={() => setEditing(true)}
      >
        <Pencil />
      </Button>
      <Button
        size="icon"
        variant="ghost"
        aria-label={`Delete limit ${limit.start_date}`}
        disabled={remove.isPending}
        onClick={() => remove.mutate()}
      >
        <Trash2 />
      </Button>
    </div>
  );
}

/** Every limit of a budget, newest first, with add, edit and delete. */
export function LimitEditor({ budgetId }: { budgetId: string }) {
  const queryClient = useQueryClient();
  const limits = useQuery(limitsQuery(budgetId));
  const currencies = useQuery(currenciesQuery);
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["budgets"] });
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold">Limits</h2>
      <div className="rounded-lg border p-3 sm:p-4">
        <LimitFormRow
          initial={thisMonth()}
          submitLabel="Add limit"
          save={(form) =>
            call(
              api.POST("/api/v1/budgets/{id}/limits", {
                params: { path: { id: budgetId } },
                body: form,
              }),
            )
          }
          onDone={() => void refresh()}
        />
      </div>
      <QueryState isPending={limits.isPending} error={limits.error} />
      {limits.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No limits yet.</p>
      ) : null}
      <div className="flex flex-col gap-4">
        {limits.data?.map((limit) => (
          <LimitRow
            key={limit.id}
            limit={limit}
            places={placesOf(currencies.data, limit.currency)}
            onChanged={refresh}
          />
        ))}
      </div>
    </section>
  );
}
