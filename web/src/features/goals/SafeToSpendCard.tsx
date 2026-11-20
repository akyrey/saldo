import { useQuery } from "@tanstack/react-query";
import { Amount } from "@/components/Amount";
import { QueryState } from "@/components/QueryState";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { isBelowZero } from "@/features/reports/forecast-model";
import { cn } from "@/lib/utils";
import { ADDING_PARTS, PART_LABELS, type SafeToSpend, safeToSpendQuery } from "./queries";

/** This month's safe-to-spend figure, per day, and how it is made up. */
export function SafeToSpendCard({ scope }: { scope?: string }) {
  const query = useQuery(safeToSpendQuery(scope));
  const data = query.data;
  return (
    <Card>
      <CardHeader>
        <CardTitle>Safe to spend this month</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <QueryState isPending={query.isPending} error={query.error} />
        {data ? <SafeToSpendBody data={data} /> : null}
      </CardContent>
    </Card>
  );
}

function SafeToSpendBody({ data }: { data: SafeToSpend }) {
  const negative = isBelowZero(data.safe_to_spend);
  return (
    <>
      <p className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <Amount
          value={data.safe_to_spend}
          currency={data.currency}
          className={cn("text-2xl font-semibold", negative && "text-destructive")}
          data-testid="safe-to-spend"
        />
        <span className="text-sm text-muted-foreground">
          <Amount value={data.per_day} currency={data.currency} /> a day for {data.days_left}{" "}
          {data.days_left === 1 ? "day" : "days"}
        </span>
      </p>
      <details className="text-sm">
        <summary className="cursor-pointer text-muted-foreground">How this is worked out</summary>
        <ul className="mt-2 divide-y">
          {Object.entries(PART_LABELS).map(([part, label]) => {
            const value = data.parts[part as keyof SafeToSpend["parts"]];
            const items = data.items.filter((i) => i.part === part);
            return (
              <li key={part} className="py-1.5">
                <div className="flex justify-between gap-2">
                  <span>
                    {ADDING_PARTS.has(part) ? "+ " : "− "}
                    {label}
                  </span>
                  <Amount value={value} currency={data.currency} />
                </div>
                {items.length > 0 ? (
                  <ul className="mt-1 pl-4 text-xs text-muted-foreground">
                    {items.map((item) => (
                      <li
                        key={`${item.source}-${item.source_id}-${item.date ?? ""}`}
                        className="flex justify-between gap-2"
                      >
                        <span>
                          {item.title}
                          {item.date ? ` (${item.date})` : ""}
                        </span>
                        <Amount value={item.amount} currency={data.currency} />
                      </li>
                    ))}
                  </ul>
                ) : null}
              </li>
            );
          })}
        </ul>
      </details>
    </>
  );
}
