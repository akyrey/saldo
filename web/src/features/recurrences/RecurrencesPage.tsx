import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Pencil, Play, Plus } from "lucide-react";
import { useState } from "react";
import { errorMessage } from "@/api/errors";
import { ActiveSwitch } from "@/components/ActiveSwitch";
import { Amount } from "@/components/Amount";
import { PageHeader } from "@/components/PageHeader";
import { InactiveSection } from "@/components/InactiveSection";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { accountsQuery } from "@/features/accounts/queries";
import { partitionByActive } from "@/features/shared/active";
import { describeRepetition, fromRecurrence, toRequest } from "./recurrence-model";
import { fireRecurrence, recurrencesQuery, replaceRecurrence, type Recurrence } from "./queries";

function RecurrenceItem({
  recurrence,
  nameOf,
}: {
  recurrence: Recurrence;
  nameOf: (id: string) => string;
}) {
  const queryClient = useQueryClient();
  const [message, setMessage] = useState<string | null>(null);
  const fire = useMutation({
    mutationFn: () => fireRecurrence(recurrence.id),
    onSuccess: async (result) => {
      setMessage(
        result.data.created
          ? `Created the transaction for ${result.data.date}.`
          : `Already created for ${result.data.date}.`,
      );
      await Promise.all(
        ["recurrences", "transactions", "balances"].map((key) =>
          queryClient.invalidateQueries({ queryKey: [key] }),
        ),
      );
    },
  });
  const toggle = useMutation({
    mutationFn: (active: boolean) =>
      replaceRecurrence(recurrence.id, toRequest({ ...fromRecurrence(recurrence), active })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["recurrences"] }),
  });
  const [first] = recurrence.splits;
  return (
    <li aria-label={recurrence.title} className="flex flex-col gap-1 rounded-lg border p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-3">
          <ActiveSwitch
            name={recurrence.title}
            checked={recurrence.active}
            disabled={toggle.isPending}
            onChange={(active) => toggle.mutate(active)}
          />
          <h2 className={cn("font-medium", !recurrence.active && "opacity-60")}>
            {recurrence.title}
          </h2>
        </div>
        {first ? (
          <span className={cn(!recurrence.active && "opacity-60")}>
            <Amount value={first.amount} currency={first.currency} />
          </span>
        ) : null}
      </div>
      {/* Inactive recurrences are dimmed; switch and buttons keep full strength. */}
      <div className={cn("flex flex-col gap-1", !recurrence.active && "opacity-60")}>
        <p className="text-sm">{recurrence.repetitions.map(describeRepetition).join("; ")}</p>
        {first ? (
          <p className="text-xs text-muted-foreground">
            {nameOf(first.source_account_id)} → {nameOf(first.destination_account_id)}
            {recurrence.splits.length > 1 ? ` (+${recurrence.splits.length - 1} splits)` : ""}
            {recurrence.last_fired_date ? ` · last created ${recurrence.last_fired_date}` : ""}
          </p>
        ) : null}
      </div>
      <div className="mt-1 flex gap-2">
        <Button size="sm" variant="outline" disabled={fire.isPending} onClick={() => fire.mutate()}>
          <Play /> Create today&apos;s transaction
        </Button>
        <Button size="sm" variant="ghost" asChild>
          <Link to="/recurrences/$recurrenceId/edit" params={{ recurrenceId: recurrence.id }}>
            <Pencil /> Edit
          </Link>
        </Button>
      </div>
      {message ? (
        <p role="status" className="text-sm text-muted-foreground">
          {message}
        </p>
      ) : null}
      {fire.error ? <Alert>{errorMessage(fire.error)}</Alert> : null}
      {toggle.error ? <Alert>{errorMessage(toggle.error)}</Alert> : null}
    </li>
  );
}

export function RecurrencesPage() {
  const [recurrences, accounts] = useQueries({ queries: [recurrencesQuery, accountsQuery()] });
  const nameOf = (id: string) => accounts.data?.find((a) => a.id === id)?.name ?? "…";
  const { active, inactive } = partitionByActive(recurrences.data ?? []);
  const items = (list: Recurrence[]) => (
    <ul className="grid gap-3">
      {list.map((r) => (
        <RecurrenceItem key={r.id} recurrence={r} nameOf={nameOf} />
      ))}
    </ul>
  );
  return (
    <div>
      <PageHeader
        title="Recurring transactions"
        actions={
          <Button size="sm" asChild>
            <Link to="/recurrences/new">
              <Plus /> New
            </Link>
          </Button>
        }
      />
      <QueryState isPending={recurrences.isPending} error={recurrences.error} />
      {recurrences.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No recurring transactions yet.</p>
      ) : null}
      {items(active)}
      <InactiveSection count={inactive.length}>{items(inactive)}</InactiveSection>
    </div>
  );
}
