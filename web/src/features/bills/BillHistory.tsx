import { useQuery } from "@tanstack/react-query";
import { Amount } from "@/components/Amount";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { currenciesQuery, placesOf } from "@/features/accounts/queries";
import { PaymentHistoryChart } from "@/features/reports/charts";
import { todayIso } from "@/lib/dates";
import { historyChart, type BillHistory } from "./history-model";

interface BillHistoryProps {
  history: BillHistory;
}

interface FigureProps {
  label: string;
  value: string | null | undefined;
  currency: string;
}

function Figure({ label, value, currency }: FigureProps) {
  return (
    <div className="flex flex-col">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="text-lg font-medium">
        {value ? <Amount value={value} currency={currency} /> : "—"}
      </dd>
    </div>
  );
}

/** Count, total, average, lowest and highest payment in the bill's currency. */
export function BillSummary({ history }: BillHistoryProps) {
  const { summary, currency } = history;
  return (
    <Card className="mb-4">
      <CardHeader>
        <CardTitle>Summary</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid grid-cols-2 gap-4 sm:grid-cols-5">
          <div className="flex flex-col">
            <dt className="text-xs text-muted-foreground">Payments</dt>
            <dd className="text-lg font-medium tabular-nums">{summary.count}</dd>
          </div>
          <Figure label="Total paid" value={summary.total} currency={currency} />
          <Figure label="Average" value={summary.average} currency={currency} />
          <Figure label="Lowest" value={summary.min} currency={currency} />
          <Figure label="Highest" value={summary.max} currency={currency} />
        </dl>
        {summary.other_currency_count > 0 ? (
          <p className="mt-3 text-sm text-muted-foreground">
            {summary.other_currency_count} payment(s) in another currency are listed below but not
            counted here.
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}

/** The payment chart over the bill's whole duration. */
export function PaymentHistory({ history }: BillHistoryProps) {
  const currencies = useQuery(currenciesQuery);
  const chart = historyChart(history, placesOf(currencies.data, history.currency), todayIso());
  const count = chart.payments.length;
  const label =
    `Payments from ${history.range.from} to ${history.range.to}: ${count} in ${history.currency}, ` +
    `expected between ${history.amount_min} and ${history.amount_max}.`;
  return (
    <Card className="mb-4">
      <CardHeader>
        <CardTitle>Payment history</CardTitle>
      </CardHeader>
      <CardContent>
        {count === 0 ? (
          <p className="mb-2 text-sm text-muted-foreground">
            No transactions linked to this bill yet.
          </p>
        ) : null}
        <PaymentHistoryChart chart={chart} currency={history.currency} label={label} />
        {history.truncated ? (
          <p className="mt-2 text-sm text-muted-foreground">
            Only the latest 2 000 due dates are shown.
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}
