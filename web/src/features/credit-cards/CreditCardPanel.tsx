import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { CreditCard } from "lucide-react";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Amount } from "@/components/Amount";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { NativeSelect } from "@/components/ui/native-select";
import {
  accountsQuery,
  currenciesQuery,
  placesOf,
  type Account,
} from "@/features/accounts/queries";
import { byId } from "@/features/shared/lookup";
import { cn } from "@/lib/utils";
import { CardBudgetForm, CardPlanForm } from "./CardPlanForm";
import {
  cardProgress,
  hasDifference,
  monthLabel,
  settlementStateLabel,
  shiftMonth,
  shortMonth,
} from "./credit-card-model";
import {
  cardCycleQuery,
  cardCyclesQuery,
  cardPlanQuery,
  type CardCycle,
  type CardPlan,
  type CardSplit,
} from "./queries";

type Editing = "none" | "plan" | "budget";

/** A credit card's monthly plan: what is left this month, the payment, and moves. */
export function CreditCardPanel({ card }: { card: Account }) {
  const plan = useQuery(cardPlanQuery(card.id));
  const [editing, setEditing] = useState<Editing>("none");
  if (plan.data === undefined) return <QueryState isPending={plan.isPending} error={plan.error} />;
  return (
    <Card className="mb-6" data-testid="credit-card-panel">
      <CardHeader className="flex-row flex-wrap items-baseline justify-between gap-2">
        <CardTitle className="flex items-center gap-2">
          <CreditCard className="size-4" aria-hidden /> Monthly plan
        </CardTitle>
        {plan.data && editing === "none" ? (
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={() => setEditing("budget")}>
              Change budget
            </Button>
            <Button variant="outline" size="sm" onClick={() => setEditing("plan")}>
              Edit plan
            </Button>
          </div>
        ) : null}
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {plan.data ? (
          <PlanBody card={card} plan={plan.data} editing={editing} setEditing={setEditing} />
        ) : editing === "plan" ? (
          <CardPlanForm card={card} plan={null} onDone={() => setEditing("none")} />
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
            <p className="text-muted-foreground">
              Set a monthly budget to see what is left, and let Saldo book the payment from your
              bank account.
            </p>
            <Button size="sm" onClick={() => setEditing("plan")}>
              Set up a monthly plan
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

interface PlanBodyProps {
  card: Account;
  plan: CardPlan;
  editing: Editing;
  setEditing: (editing: Editing) => void;
}

function PlanBody({ card, plan, editing, setEditing }: PlanBodyProps) {
  const cycles = useQuery(cardCyclesQuery(card.id));
  const [chosen, setChosen] = useState<string | null>(null);
  const current = cycles.data?.[0]?.cycle ?? plan.starts_cycle;
  const cycle = chosen ?? current;
  const months = [shiftMonth(current, 1), ...(cycles.data ?? []).map((c) => c.cycle)];
  const done = () => setEditing("none");
  return (
    <>
      {editing === "plan" ? <CardPlanForm card={card} plan={plan} onDone={done} /> : null}
      {editing === "budget" ? (
        <CardBudgetForm card={card} plan={plan} currentCycle={current} onDone={done} />
      ) : null}
      <label className="flex items-center gap-2 text-sm">
        <span className="text-muted-foreground">Month</span>
        <NativeSelect
          aria-label="Month"
          className="w-auto"
          value={cycle}
          onChange={(e) => setChosen(e.target.value)}
        >
          {months.map((m) => (
            <option key={m} value={m}>
              {monthLabel(m)}
            </option>
          ))}
        </NativeSelect>
      </label>
      <CycleDetail card={card} plan={plan} cycle={cycle} />
    </>
  );
}

function CycleDetail({ card, plan, cycle }: { card: Account; plan: CardPlan; cycle: string }) {
  const detail = useQuery(cardCycleQuery(card.id, cycle));
  if (!detail.data) return <QueryState isPending={detail.isPending} error={detail.error} />;
  return (
    <>
      <CycleSummary plan={plan} cycle={detail.data} />
      <CycleSplits card={card} cycle={cycle} splits={detail.data.splits} />
    </>
  );
}

function CycleSummary({ plan, cycle }: { plan: CardPlan; cycle: CardCycle }) {
  const currencies = useQuery(currenciesQuery);
  const accounts = useQuery(accountsQuery());
  const places = placesOf(currencies.data, cycle.currency);
  const { percent, overspent } = cardProgress(cycle.spent, cycle.limit, places);
  const payer = byId(accounts.data).get(plan.settlement_account_id)?.name ?? "the paying account";
  const { settlement } = cycle;
  return (
    <div className="flex flex-col gap-2 text-sm">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <span>
          <Amount value={cycle.spent} currency={cycle.currency} /> of{" "}
          <Amount value={cycle.limit} currency={cycle.currency} />
        </span>
        <span className={cn(overspent ? "text-destructive" : "font-semibold")}>
          {overspent ? "Over by " : "Left: "}
          <Amount
            value={cycle.available.replace(/^-/, "")}
            currency={cycle.currency}
            data-testid="card-available"
          />
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={`Spent ${cycle.spent} of ${cycle.limit} ${cycle.currency}`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percent)}
        className="h-2 overflow-hidden rounded-full bg-muted"
      >
        <div
          className={cn("h-full rounded-full", overspent ? "bg-destructive" : "bg-primary")}
          style={{ width: `${percent}%` }}
        />
      </div>
      <p className="text-muted-foreground" data-testid="card-settlement">
        {settlement.amount && settlement.group_id ? (
          <>
            Payment: <Amount value={settlement.amount} currency={cycle.currency} /> from {payer} on{" "}
            <Link
              to="/transactions/$groupId/edit"
              params={{ groupId: settlement.group_id }}
              className="underline"
            >
              {settlement.date}
            </Link>
          </>
        ) : (
          <>No payment yet; due on {settlement.date}</>
        )}{" "}
        <Badge>{settlementStateLabel(settlement.state)}</Badge>
      </p>
      {hasDifference(cycle.difference_after_settlement, places) ? (
        <p className="text-destructive">
          Changed after the payment:{" "}
          <Amount value={cycle.difference_after_settlement ?? "0"} currency={cycle.currency} /> not
          paid. Move those transactions to the next month or adjust the payment.
        </p>
      ) : null}
    </div>
  );
}

function CycleSplits({
  card,
  cycle,
  splits,
}: {
  card: Account;
  cycle: string;
  splits: CardSplit[];
}) {
  const queryClient = useQueryClient();
  const move = useMutation({
    mutationFn: ({ split, back }: { split: string; back: boolean }) => {
      const params = { path: { id: card.id, split_id: split } };
      const path = "/api/v1/accounts/{id}/credit-card/moves/{split_id}" as const;
      return call(back ? api.DELETE(path, { params }) : api.PUT(path, { params }));
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["credit-cards"] }),
  });
  if (splits.length === 0) {
    return <p className="text-sm text-muted-foreground">No card transactions in this month.</p>;
  }
  return (
    <div className="flex flex-col gap-2">
      {move.error ? <Alert>{errorMessage(move.error)}</Alert> : null}
      <ul className="divide-y rounded-md border text-sm" aria-label="Card transactions">
        {splits.map((s) => (
          <li
            key={s.split_id}
            className="flex flex-wrap items-center justify-between gap-2 px-3 py-2"
          >
            <span className={cn("min-w-0", s.moved_out && "text-muted-foreground line-through")}>
              <span className="tabular-nums text-muted-foreground">{s.date}</span>{" "}
              <Link
                to="/transactions/$groupId/edit"
                params={{ groupId: s.group_id }}
                className="hover:underline"
              >
                {s.description}
              </Link>{" "}
              <span className="text-muted-foreground">· {s.counterparty.name}</span>
              {s.moved_in ? (
                <Badge className="ml-1">from {shortMonth(shiftMonth(cycle, -1))}</Badge>
              ) : null}
              {s.moved_out ? (
                <Badge className="ml-1">to {shortMonth(shiftMonth(cycle, 1))}</Badge>
              ) : null}
            </span>
            <span className="flex items-center gap-2">
              <Amount value={s.amount} currency={card.currency} />
              <Button
                variant="ghost"
                size="sm"
                disabled={move.isPending}
                onClick={() => move.mutate({ split: s.split_id, back: s.moved_in || s.moved_out })}
              >
                {s.moved_in || s.moved_out ? "Move back" : "Count in next month"}
              </Button>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
