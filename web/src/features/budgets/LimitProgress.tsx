import { Amount } from "@/components/Amount";
import { cn } from "@/lib/utils";
import { isNegative, toMinor } from "@/api/money";
import { limitTotal, progress } from "./budget-model";
import type { BudgetLimit } from "./queries";

interface LimitProgressProps {
  limit: BudgetLimit;
  places: number;
}

/** One limit as a bar: what was spent of the amount (plus any carry), and what is left. */
export function LimitProgress({ limit, places }: LimitProgressProps) {
  const total = limitTotal(limit, places);
  const { percent, overspent } = progress(limit.spent, total, places);
  const hasCarry = (toMinor(limit.carried, places) ?? 0n) !== 0n;
  return (
    <div className="flex flex-col gap-1" data-testid={`limit-${limit.id}`}>
      <div className="flex flex-wrap items-baseline justify-between gap-2 text-sm">
        <span className="text-muted-foreground">
          {limit.start_date} – {limit.end_date}
        </span>
        <span>
          <Amount value={limit.spent} currency={limit.currency} /> of{" "}
          <Amount value={total} currency={limit.currency} />
          {hasCarry ? (
            <span className="text-xs text-muted-foreground">
              {" "}
              (<Amount value={limit.amount} currency={limit.currency} />{" "}
              {isNegative(limit.carried) ? "−" : "+"}{" "}
              <Amount
                value={limit.carried.replace(/^-/, "")}
                currency={limit.currency}
                className={isNegative(limit.carried) ? "text-destructive" : undefined}
              />{" "}
              carried)
            </span>
          ) : null}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={`Spent ${limit.spent} of ${total} ${limit.currency}`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        className="h-2 overflow-hidden rounded-full bg-muted"
      >
        <div
          className={cn("h-full rounded-full", overspent ? "bg-destructive" : "bg-primary")}
          style={{ width: `${percent}%` }}
        />
      </div>
      <p className={cn("text-xs", overspent ? "text-destructive" : "text-muted-foreground")}>
        {overspent ? "Over by " : "Left: "}
        <Amount
          value={overspent ? limit.available.replace(/^-/, "") : limit.available}
          currency={limit.currency}
          className={overspent ? "text-destructive" : undefined}
        />
      </p>
    </div>
  );
}
