import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { normalizeAmountInput } from "@/api/money";
import { Amount } from "@/components/Amount";
import { DatePicker } from "@/components/DatePicker";
import { Field } from "@/components/Field";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { accountQuery, currenciesQuery, placesOf } from "@/features/accounts/queries";
import { todayIso } from "@/lib/dates";
import { draftQuery, type ReconciliationDraft } from "./queries";
import { tally } from "./reconcile-model";

/** What a finished reconciliation changes. */
const AFFECTED = [["reconciliations"], ["transactions"], ["balances"], ["accounts"], ["reports"]];

interface ChecklistProps {
  draft: ReconciliationDraft;
  selected: ReadonlySet<string>;
  onToggle: (id: string) => void;
  onAll: (all: boolean) => void;
}

function Checklist({ draft, selected, onToggle, onAll }: ChecklistProps) {
  if (draft.splits.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        Every transaction up to this date is reconciled.
      </p>
    );
  }
  const all = draft.splits.every((s) => selected.has(s.id));
  return (
    <div className="flex flex-col gap-2">
      <label className="flex items-center gap-2 text-sm font-medium">
        <input type="checkbox" checked={all} onChange={(e) => onAll(e.target.checked)} />
        Select all {draft.splits.length} up to {draft.statement_date}
      </label>
      <ul className="divide-y rounded-md border text-sm">
        {draft.splits.map((split) => (
          <li key={split.id}>
            <label className="flex items-center gap-3 px-3 py-2">
              <input
                type="checkbox"
                checked={selected.has(split.id)}
                onChange={() => onToggle(split.id)}
                aria-label={`${split.date} ${split.description}`}
              />
              <span className="w-24 shrink-0 text-muted-foreground">{split.date}</span>
              <span className="min-w-0 flex-1 truncate">{split.description}</span>
              <Amount value={split.amount} currency={draft.currency} />
            </label>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Ticks off an account's transactions against a bank statement. */
export function ReconcilePage() {
  const { accountId } = useParams({ from: "/app/accounts/$accountId/reconcile" });
  const search = useSearch({ from: "/app/accounts/$accountId/reconcile" });
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [date, setDate] = useState(search.date ?? todayIso());
  const [balance, setBalance] = useState(search.balance ?? "");
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const account = useQuery(accountQuery(accountId));
  const currencies = useQuery(currenciesQuery).data;
  const draft = useQuery(draftQuery(accountId, date));
  const finish = useMutation({
    mutationFn: (book: boolean) =>
      call(
        api.POST("/api/v1/accounts/{id}/reconciliations", {
          params: { path: { id: accountId } },
          body: {
            statement_date: date,
            statement_balance: normalizeAmountInput(balance),
            split_ids: [...selected].filter((id) => draft.data?.splits.some((s) => s.id === id)),
            book_difference: book,
          },
        }),
      ),
    onSuccess: async () => {
      await Promise.all(AFFECTED.map((queryKey) => queryClient.invalidateQueries({ queryKey })));
      await navigate({ to: "/accounts/$accountId", params: { accountId } });
    },
  });
  const errors = fieldErrors(finish.error);
  const data = draft.data;
  const places = placesOf(currencies, data?.currency ?? "");
  const totals = data ? tally(data.cleared_balance, data.splits, selected, balance, places) : null;
  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={`Reconcile ${account.data?.name ?? ""}`}
        actions={
          <Button variant="outline" size="sm" asChild>
            <Link to="/accounts/$accountId" params={{ accountId }}>
              Back to account
            </Link>
          </Button>
        }
      />
      <div className="grid gap-3 sm:grid-cols-2">
        <Field id="statement-date" label="Statement date" error={errors.statement_date}>
          <DatePicker id="statement-date" value={date} onChange={(v) => v && setDate(v)} />
        </Field>
        <Field id="statement-balance" label="Statement balance" error={errors.statement_balance}>
          <Input
            id="statement-balance"
            inputMode="decimal"
            autoComplete="off"
            placeholder="0.00"
            value={balance}
            onChange={(e) => setBalance(e.target.value)}
          />
        </Field>
      </div>
      {data?.reconciled_through ? (
        <p className="text-sm text-muted-foreground">Reconciled up to {data.reconciled_through}.</p>
      ) : null}
      <QueryState isPending={draft.isPending} error={draft.error} />
      {data ? (
        <Checklist
          draft={data}
          selected={selected}
          onToggle={toggle}
          onAll={(all) => setSelected(all ? new Set(data.splits.map((s) => s.id)) : new Set())}
        />
      ) : null}
      {data && totals ? (
        <dl className="grid grid-cols-2 gap-3 rounded-lg border p-4 text-sm sm:grid-cols-3">
          <div>
            <dt className="text-muted-foreground">Cleared</dt>
            <dd data-testid="reconcile-cleared">
              <Amount value={totals.cleared} currency={data.currency} />
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Difference</dt>
            <dd
              data-testid="reconcile-difference"
              className={totals.balanced ? undefined : "text-destructive"}
            >
              {totals.difference === null ? (
                "—"
              ) : (
                <Amount value={totals.difference} currency={data.currency} />
              )}
            </dd>
          </div>
        </dl>
      ) : null}
      {finish.error && Object.keys(errors).length === 0 ? (
        <Alert>{errorMessage(finish.error)}</Alert>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button
          disabled={!totals?.balanced || finish.isPending}
          onClick={() => finish.mutate(false)}
        >
          Finish
        </Button>
        <Button
          variant="outline"
          disabled={!totals || totals.difference === null || totals.balanced || finish.isPending}
          onClick={() => finish.mutate(true)}
        >
          Book difference and finish
        </Button>
      </div>
    </div>
  );
}
