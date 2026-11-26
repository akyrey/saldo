import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { normalizeAmountInput } from "@/api/money";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { accountsQuery, type Account } from "@/features/accounts/queries";
import { budgetMonthChoices, monthLabel, payingAccounts } from "./credit-card-model";
import type { CardPlan } from "./queries";

const AFFECTED = [["credit-cards"], ["transactions"], ["balances"], ["accounts"], ["reports"]];

function useInvalidate() {
  const queryClient = useQueryClient();
  return () => Promise.all(AFFECTED.map((queryKey) => queryClient.invalidateQueries({ queryKey })));
}

interface CardPlanFormProps {
  card: Account;
  plan: CardPlan | null;
  onDone: () => void;
}

/** Creates or changes a card's plan: budget (new plans), paying account, payment day. */
export function CardPlanForm({ card, plan, onDone }: CardPlanFormProps) {
  const accounts = useQuery(accountsQuery());
  const invalidate = useInvalidate();
  const [limit, setLimit] = useState("");
  const [payer, setPayer] = useState(plan?.settlement_account_id ?? "");
  const [day, setDay] = useState(String(plan?.settlement_day ?? 15));
  const [confirmRemove, setConfirmRemove] = useState(false);
  const save = useMutation({
    mutationFn: () =>
      call(
        api.PUT("/api/v1/accounts/{id}/credit-card", {
          params: { path: { id: card.id } },
          body: {
            monthly_limit: plan ? null : normalizeAmountInput(limit),
            settlement_account_id: payer,
            settlement_day: Number(day),
          },
        }),
      ),
    onSuccess: async () => {
      await invalidate();
      onDone();
    },
  });
  const remove = useMutation({
    mutationFn: () =>
      call(api.DELETE("/api/v1/accounts/{id}/credit-card", { params: { path: { id: card.id } } })),
    onSuccess: async () => {
      await invalidate();
      onDone();
    },
  });
  const errors = fieldErrors(save.error);
  const choices = payingAccounts(accounts.data ?? [], card);

  return (
    <form
      aria-label="Monthly plan"
      className="grid gap-3 sm:grid-cols-3"
      onSubmit={(event) => {
        event.preventDefault();
        save.mutate();
      }}
    >
      {plan ? null : (
        <Field
          id="card-limit"
          label={`Monthly budget (${card.currency})`}
          error={errors.monthly_limit}
        >
          <Input
            id="card-limit"
            inputMode="decimal"
            required
            value={limit}
            onChange={(e) => setLimit(e.target.value)}
            {...errorProps("card-limit", errors.monthly_limit)}
          />
        </Field>
      )}
      <Field id="card-payer" label="Paid from" error={errors.settlement_account_id}>
        <NativeSelect
          id="card-payer"
          required
          value={payer}
          onChange={(e) => setPayer(e.target.value)}
          {...errorProps("card-payer", errors.settlement_account_id)}
        >
          <option value="">Choose an account</option>
          {choices.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="card-day" label="Payment day of the next month" error={errors.settlement_day}>
        <Input
          id="card-day"
          type="number"
          min={1}
          max={31}
          required
          value={day}
          onChange={(e) => setDay(e.target.value)}
          {...errorProps("card-day", errors.settlement_day)}
        />
      </Field>
      <p className="text-xs text-muted-foreground sm:col-span-3">
        In a shorter month the card is paid on its last day.
      </p>
      {save.error && Object.keys(errors).length === 0 ? (
        <Alert className="sm:col-span-3">{errorMessage(save.error)}</Alert>
      ) : null}
      {remove.error ? <Alert className="sm:col-span-3">{errorMessage(remove.error)}</Alert> : null}
      <div className="flex flex-wrap gap-2 sm:col-span-3">
        <Button type="submit" size="sm" disabled={save.isPending}>
          {plan ? "Save plan" : "Set up plan"}
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onDone}>
          Cancel
        </Button>
        {plan ? (
          <Button
            type="button"
            size="sm"
            variant={confirmRemove ? "destructive" : "outline"}
            disabled={remove.isPending}
            onClick={() => (confirmRemove ? remove.mutate() : setConfirmRemove(true))}
          >
            {confirmRemove ? "Remove plan and its upcoming payments?" : "Remove plan"}
          </Button>
        ) : null}
      </div>
    </form>
  );
}

interface CardBudgetFormProps {
  card: Account;
  plan: CardPlan;
  currentCycle: string;
  onDone: () => void;
}

/** Sets a new budget from a month on; earlier months keep theirs. */
export function CardBudgetForm({ card, plan, currentCycle, onDone }: CardBudgetFormProps) {
  const invalidate = useInvalidate();
  const [amount, setAmount] = useState("");
  const [from, setFrom] = useState(currentCycle);
  const save = useMutation({
    mutationFn: () =>
      call(
        api.PUT("/api/v1/accounts/{id}/credit-card/limits/{cycle}", {
          params: { path: { id: card.id, cycle: from } },
          body: { amount: normalizeAmountInput(amount) },
        }),
      ),
    onSuccess: async () => {
      await invalidate();
      onDone();
    },
  });
  const errors = fieldErrors(save.error);
  return (
    <form
      aria-label="Change budget"
      className="grid gap-3 sm:grid-cols-3"
      onSubmit={(event) => {
        event.preventDefault();
        save.mutate();
      }}
    >
      <Field
        id="budget-amount"
        label={`New budget (${card.currency})`}
        error={errors.monthly_limit}
      >
        <Input
          id="budget-amount"
          inputMode="decimal"
          required
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          {...errorProps("budget-amount", errors.monthly_limit)}
        />
      </Field>
      <Field id="budget-from" label="From" error={errors.cycle}>
        <NativeSelect id="budget-from" value={from} onChange={(e) => setFrom(e.target.value)}>
          {budgetMonthChoices(currentCycle).map((cycle) => (
            <option key={cycle} value={cycle}>
              {monthLabel(cycle)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <div className="flex items-end gap-2">
        <Button type="submit" size="sm" disabled={save.isPending}>
          Change budget
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onDone}>
          Cancel
        </Button>
      </div>
      {save.error && Object.keys(errors).length === 0 ? (
        <Alert className="sm:col-span-3">{errorMessage(save.error)}</Alert>
      ) : null}
      <ul className="text-xs text-muted-foreground sm:col-span-3">
        {plan.limits.map((l) => (
          <li key={l.from_cycle}>
            From {monthLabel(l.from_cycle)}: {l.amount} {card.currency}
          </li>
        ))}
      </ul>
    </form>
  );
}
