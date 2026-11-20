// Views of the Phase 10 reports: savings rate, year over year, averages.
import type { ReactNode } from "react";
import { Amount } from "@/components/Amount";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { placesOf, type Currency } from "@/features/accounts/queries";
import { RankedBars, ValueLine, YearLines } from "./charts";
import type { CashFlowReport } from "./queries";
import { intervalLabel, plotValue, type Interval } from "./report-model";
import {
  type CategoryAveragesReport,
  formatBp,
  MONTH_NAMES,
  type YearOverYearReport,
  yearRows,
} from "./trend-model";

interface ViewProps<T> {
  report: T;
  currencies: readonly Currency[] | undefined;
}

function Empty() {
  return <p className="text-sm text-muted-foreground">Nothing in this period.</p>;
}

function Section({ currency, children }: { currency: string; children: ReactNode }) {
  return (
    <section aria-label={currency} className="flex flex-col gap-3 rounded-lg border p-3 sm:p-4">
      <h2 className="text-sm font-semibold">{currency}</h2>
      {children}
    </section>
  );
}

/** A plain table whose cells are already rendered (amounts, percentages). */
function TextTable({
  caption,
  head,
  rows,
}: {
  caption: string;
  head: readonly string[];
  rows: readonly (readonly ReactNode[])[];
}) {
  return (
    <div className="overflow-x-auto">
      <Table>
        <caption className="sr-only">{caption}</caption>
        <TableHeader>
          <TableRow>
            {head.map((title, i) => (
              <TableHead key={title} className={i === 0 ? undefined : "text-right"}>
                {title}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((cells, r) => (
            <TableRow key={r}>
              {cells.map((cell, c) => (
                <TableCell key={c} className={c === 0 ? undefined : "text-right tabular-nums"}>
                  {cell}
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

/** Saved (income minus expenses) per interval, with the share of income kept. */
export function SavingsRateView({ report, currencies }: ViewProps<CashFlowReport>) {
  const interval = report.range.interval as Interval;
  if (report.currencies.length === 0) return <Empty />;
  return report.currencies.map((c) => {
    const places = placesOf(currencies, c.currency);
    return (
      <Section key={c.currency} currency={c.currency}>
        <p className="text-sm">
          Saved <Amount value={c.net} currency={c.currency} /> of{" "}
          <Amount value={c.income} currency={c.currency} /> income:{" "}
          <span className="font-semibold" data-testid={`savings-rate-${c.currency}`}>
            {formatBp(c.savings_rate_bp)}
          </span>
        </p>
        <ValueLine
          rows={c.intervals.map((i) => ({
            label: intervalLabel(i.start, interval),
            first: plotValue(i.net, places),
            second: 0,
            firstText: i.net,
            secondText: i.net,
          }))}
          currency={c.currency}
          names={["Saved", "Saved"]}
          label={`Saved in ${c.currency} per ${interval}`}
        />
        <TextTable
          caption={`Savings rate in ${c.currency}`}
          head={["Period", "Income", "Expenses", "Saved", "Rate"]}
          rows={c.intervals.map((i) => [
            intervalLabel(i.start, interval),
            <Amount key="i" value={i.income} currency={c.currency} />,
            <Amount key="e" value={i.expenses} currency={c.currency} />,
            <Amount key="n" value={i.net} currency={c.currency} />,
            formatBp(i.savings_rate_bp),
          ])}
        />
      </Section>
    );
  });
}

/** Lines per year over January to December, and the month-by-month table. */
export function YearOverYearView({ report, currencies }: ViewProps<YearOverYearReport>) {
  if (report.currencies.length === 0) return <Empty />;
  return report.currencies.map((c) => {
    const years = c.years.map((y) => y.year);
    const last = c.years.at(-1);
    return (
      <Section key={c.currency} currency={c.currency}>
        {last ? (
          <p className="text-sm">
            {last.year} so far: <Amount value={last.total} currency={c.currency} />
            {last.change ? (
              <>
                {" "}
                (<Amount value={last.change} currency={c.currency} signed />,{" "}
                {formatBp(last.change_bp)} against the same months of {last.year - 1})
              </>
            ) : null}
          </p>
        ) : null}
        <YearLines
          rows={yearRows(c.years, placesOf(currencies, c.currency))}
          years={years}
          label={`${report.metric} in ${c.currency} per month, one line per year`}
        />
        <TextTable
          caption={`Year over year in ${c.currency}`}
          head={["Month", ...years.map(String), "Change"]}
          rows={MONTH_NAMES.map((name, m) => [
            name,
            ...c.years.map((y) => {
              const value = y.months[m];
              return value ? <Amount key={y.year} value={value} currency={c.currency} /> : "—";
            }),
            formatBp(last?.month_change_bp[m]),
          ])}
        />
      </Section>
    );
  });
}

/** The average month per category against this month so far. */
export function CategoryAveragesView({ report, currencies }: ViewProps<CategoryAveragesReport>) {
  if (report.currencies.length === 0) return <Empty />;
  return (
    <>
      <p className="text-sm text-muted-foreground">
        Averages over {report.months} complete months, {report.from} to {report.to}; months without
        spending count as zero.
      </p>
      {report.currencies.map((c) => {
        const places = placesOf(currencies, c.currency);
        return (
          <Section key={c.currency} currency={c.currency}>
            <RankedBars
              rows={c.items.slice(0, 10).map((i) => ({
                label: i.name ?? "(no category)",
                first: plotValue(i.average, places),
                second: plotValue(i.this_month, places),
                firstText: i.average,
                secondText: i.this_month,
              }))}
              currency={c.currency}
              names={["Average month", "This month"]}
              label={`Average monthly spending and this month in ${c.currency}`}
            />
            <TextTable
              caption={`Average monthly spending in ${c.currency}`}
              head={["Category", "Average", "Lowest", "Highest", "This month", "Against average"]}
              rows={c.items.map((i) => [
                i.name ?? "(no category)",
                <Amount key="a" value={i.average} currency={c.currency} />,
                <Amount key="l" value={i.min} currency={c.currency} />,
                <Amount key="h" value={i.max} currency={c.currency} />,
                <Amount key="t" value={i.this_month} currency={c.currency} />,
                formatBp(i.this_month_vs_average_bp),
              ])}
            />
          </Section>
        );
      })}
    </>
  );
}
