import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { useState } from "react";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { explain } from "@/api/errors";
import { Amount } from "@/components/Amount";
import { DatePicker } from "@/components/DatePicker";
import { Field } from "@/components/Field";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { accountQuery } from "@/features/accounts/queries";
import { addDays, todayIso } from "@/lib/dates";
import { proposalOf } from "./review-model";

type Candidate = components["schemas"]["EnrichmentCandidate"];

const DAYS_SHOWN_BY_DEFAULT = 365;

/** Existing transactions that stored Amazon or PayPal rows explain (Track E). */
export function EnrichPage() {
  const { accountId } = useParams({ from: "/app/accounts/$accountId/enrich" });
  const queryClient = useQueryClient();
  const [range, setRange] = useState(() => {
    const today = todayIso();
    return { from: addDays(today, -DAYS_SHOWN_BY_DEFAULT), to: today };
  });
  const { from, to } = range;
  const [chosen, setChosen] = useState<Record<string, { split: boolean }>>({});
  // A new range shows other transactions, so the old selection goes.
  const changeRange = (next: Partial<typeof range>) => {
    setRange((prev) => ({ ...prev, ...next }));
    setChosen({});
  };
  const account = useQuery(accountQuery(accountId));
  const candidates = useQuery({
    queryKey: ["sources", "enrichment", accountId, from, to],
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/accounts/{id}/enrichment", {
            params: { path: { id: accountId }, query: { from, to } },
          }),
        )
      ).data,
  });
  const accept = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/v1/accounts/{id}/enrichment/accept", {
          params: { path: { id: accountId } },
          body: {
            items: Object.entries(chosen).map(([splitId, choice]) => ({
              split_id: splitId,
              split_by_item: choice.split,
            })),
          },
        }),
      ),
    onSuccess: async () => {
      setChosen({});
      await Promise.all(
        [["sources"], ["transactions"], ["reports"]].map((queryKey) =>
          queryClient.invalidateQueries({ queryKey }),
        ),
      );
    },
  });
  const toggle = (candidate: Candidate, on: boolean) =>
    setChosen((prev) =>
      on
        ? { ...prev, [candidate.split_id]: { split: false } }
        : Object.fromEntries(Object.entries(prev).filter(([id]) => id !== candidate.split_id)),
    );
  const count = Object.keys(chosen).length;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={account.data ? `Enrich ${account.data.name}` : "Enrich from sources"}
        actions={
          <Button variant="outline" size="sm" asChild>
            <Link to="/accounts/$accountId" params={{ accountId }}>
              Back to account
            </Link>
          </Button>
        }
      />
      <p className="text-sm text-muted-foreground">
        Transactions already here that your stored Amazon and PayPal rows explain. Accepting one
        sets its description from the source and keeps the old text in its notes.
      </p>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field id="enrich-from" label="From">
          <DatePicker id="enrich-from" value={from} onChange={(v) => v && changeRange({ from: v })} />
        </Field>
        <Field id="enrich-to" label="To">
          <DatePicker id="enrich-to" value={to} onChange={(v) => v && changeRange({ to: v })} />
        </Field>
      </div>
      <QueryState isPending={candidates.isPending} error={candidates.error} />
      {candidates.data && candidates.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">Nothing to enrich in these dates.</p>
      ) : null}
      {candidates.data && candidates.data.length > 0 ? (
        <ul className="divide-y rounded-md border text-sm">
          {candidates.data.map((candidate) => {
            const proposal = proposalOf(candidate.proposal);
            const label = `${candidate.date} ${candidate.description}`;
            const choice = chosen[candidate.split_id];
            return (
              <li key={candidate.split_id} className="flex flex-wrap items-center gap-3 px-3 py-2">
                <input
                  type="checkbox"
                  aria-label={`Accept ${label}`}
                  checked={Boolean(choice)}
                  onChange={(e) => toggle(candidate, e.target.checked)}
                />
                <span className="w-24 shrink-0 text-muted-foreground">{candidate.date}</span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-muted-foreground">
                    {candidate.description}
                  </span>
                  <span className="block truncate font-medium">{proposal?.description}</span>
                </span>
                {proposal && proposal.items.length > 1 ? (
                  <label className="flex items-center gap-1 text-xs">
                    <input
                      type="checkbox"
                      aria-label={`Split ${label} by item`}
                      disabled={!choice}
                      checked={choice?.split ?? false}
                      onChange={(e) =>
                        setChosen((prev) => ({
                          ...prev,
                          [candidate.split_id]: { split: e.target.checked },
                        }))
                      }
                    />
                    Split by item
                  </label>
                ) : null}
                <Amount value={candidate.amount} currency={account.data?.currency ?? "EUR"} />
              </li>
            );
          })}
        </ul>
      ) : null}
      {accept.error ? <Alert>{explain(accept.error)}</Alert> : null}
      <div>
        <Button disabled={count === 0 || accept.isPending} onClick={() => accept.mutate()}>
          Accept {count} selected
        </Button>
      </div>
    </div>
  );
}
