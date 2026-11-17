import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearch } from "@tanstack/react-router";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { Amount } from "@/components/Amount";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { candidatesQuery, type SubscriptionCandidate } from "./queries";
import {
  acceptRequest,
  frequencyLabel,
  suggestionForm,
  type SuggestionForm,
} from "./suggestion-model";

/** Bills, suggestions and transactions all change when one is accepted. */
const AFFECTED = [["subscriptions"], ["bills"], ["transactions"], ["reports"]];

interface AddFormProps {
  candidate: SubscriptionCandidate;
  onDone: () => void;
}

function AddAsBillForm({ candidate, onDone }: AddFormProps) {
  const queryClient = useQueryClient();
  const [form, setForm] = useState(() => suggestionForm(candidate));
  const accept = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/v1/subscription-candidates/accept", {
          body: acceptRequest(candidate, form),
        }),
      ),
    onSuccess: async () => {
      await Promise.all(AFFECTED.map((queryKey) => queryClient.invalidateQueries({ queryKey })));
      onDone();
    },
  });
  const errors = fieldErrors(accept.error);
  const set = (changes: Partial<SuggestionForm>) => setForm((prev) => ({ ...prev, ...changes }));
  const id = `suggestion-${candidate.account_id}-${candidate.currency}`;
  return (
    <form
      aria-label={`Add ${candidate.account_name} as a bill`}
      className="grid gap-3 sm:grid-cols-4"
      onSubmit={(e) => {
        e.preventDefault();
        accept.mutate();
      }}
    >
      <Field id={`${id}-name`} label="Bill name" error={errors.name}>
        <Input
          id={`${id}-name`}
          required
          value={form.name}
          onChange={(e) => set({ name: e.target.value })}
        />
      </Field>
      <Field id={`${id}-min`} label="Lowest amount" error={errors.amount_min}>
        <Input
          id={`${id}-min`}
          inputMode="decimal"
          value={form.amountMin}
          onChange={(e) => set({ amountMin: e.target.value })}
        />
      </Field>
      <Field id={`${id}-max`} label="Highest amount" error={errors.amount_max}>
        <Input
          id={`${id}-max`}
          inputMode="decimal"
          value={form.amountMax}
          onChange={(e) => set({ amountMax: e.target.value })}
        />
      </Field>
      <div className="flex items-end gap-2">
        <Button type="submit" size="sm" disabled={accept.isPending}>
          Create bill
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
      {accept.error && Object.keys(errors).length === 0 ? (
        <Alert className="sm:col-span-4">{errorMessage(accept.error)}</Alert>
      ) : null}
    </form>
  );
}

function SuggestionRow({ candidate }: { candidate: SubscriptionCandidate }) {
  const queryClient = useQueryClient();
  const [adding, setAdding] = useState(false);
  const dismiss = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/v1/subscription-candidates/dismiss", {
          body: { account_id: candidate.account_id, currency: candidate.currency },
        }),
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["subscriptions"] }),
  });
  const { currency } = candidate;
  return (
    <li className="flex flex-col gap-2 py-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="font-medium">{candidate.account_name}</p>
          <p className="text-xs text-muted-foreground">
            {frequencyLabel(candidate.frequency)} · {candidate.split_ids.length} payments since{" "}
            {candidate.first_payment} · next {candidate.next_due}
          </p>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-sm tabular-nums">
            <Amount value={candidate.amount_min} currency={currency} />
            {candidate.amount_min === candidate.amount_max ? null : (
              <>
                {" – "}
                <Amount value={candidate.amount_max} currency={currency} />
              </>
            )}
          </span>
          {adding ? null : (
            <>
              <Button size="sm" onClick={() => setAdding(true)}>
                Add as bill
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={dismiss.isPending}
                onClick={() => dismiss.mutate()}
                aria-label={`Dismiss ${candidate.account_name}`}
              >
                Dismiss
              </Button>
            </>
          )}
        </div>
      </div>
      {dismiss.error ? <Alert>{errorMessage(dismiss.error)}</Alert> : null}
      {adding ? <AddAsBillForm candidate={candidate} onDone={() => setAdding(false)} /> : null}
    </li>
  );
}

/** Repeating payments without a bill, with "Add as bill" and "Dismiss". */
export function SubscriptionSuggestions() {
  const { scope } = useSearch({ from: "/app" });
  const candidates = useQuery(candidatesQuery(scope));
  const list = candidates.data ?? [];
  if (list.length === 0) return null;
  return (
    <Card className="mb-4">
      <CardHeader>
        <CardTitle>Suggestions</CardTitle>
        <p className="text-sm text-muted-foreground">Payments that repeat but have no bill yet.</p>
      </CardHeader>
      <CardContent>
        <ul className="divide-y">
          {list.map((candidate) => (
            <SuggestionRow
              key={`${candidate.account_id}-${candidate.currency}`}
              candidate={candidate}
            />
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
