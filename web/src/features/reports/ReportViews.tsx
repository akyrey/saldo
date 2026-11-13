// One view per report kind: per currency, a chart and the table behind it.
import { Amount } from "@/components/Amount";
import { placesOf, type Currency } from "@/features/accounts/queries";
import { PairedBars, RankedBars, ValueLine } from "./charts";
import type { BreakdownReport, BudgetReport, CashFlowReport, NetWorthReport } from "./queries";
import {
  breakdownRows,
  budgetRows,
  cashFlowRows,
  intervalLabel,
  netWorthRows,
  type Interval,
} from "./report-model";
import { ReportTable } from "./ReportTable";

interface ViewProps<T> {
  report: T;
  currencies: readonly Currency[] | undefined;
}

function Empty() {
  return <p className="text-sm text-muted-foreground">Nothing in this period.</p>;
}

function Section({ currency, children }: { currency: string; children: React.ReactNode }) {
  return (
    <section aria-label={currency} className="flex flex-col gap-3 rounded-lg border p-3 sm:p-4">
      <h2 className="text-sm font-semibold">{currency}</h2>
      {children}
    </section>
  );
}

export function CashFlowView({ report, currencies }: ViewProps<CashFlowReport>) {
  const interval = report.range.interval as Interval;
  if (report.currencies.length === 0) return <Empty />;
  return report.currencies.map((c) => (
    <Section key={c.currency} currency={c.currency}>
      <p className="text-sm">
        Income <Amount value={c.income} currency={c.currency} /> · Expenses{" "}
        <Amount value={c.expenses} currency={c.currency} /> · Net{" "}
        <Amount value={c.net} currency={c.currency} data-testid={`net-${c.currency}`} />
      </p>
      <PairedBars
        rows={cashFlowRows(c, interval, placesOf(currencies, c.currency))}
        currency={c.currency}
        names={["Income", "Expenses"]}
        label={`Income and expenses in ${c.currency} per ${interval}`}
      />
      <ReportTable
        caption={`Cash flow in ${c.currency}`}
        firstTitle="Period"
        labels={c.intervals.map((i) => intervalLabel(i.start, interval))}
        columns={[
          { title: "Income", values: c.intervals.map((i) => i.income) },
          { title: "Expenses", values: c.intervals.map((i) => i.expenses) },
          { title: "Net", values: c.intervals.map((i) => i.net) },
        ]}
        currency={c.currency}
      />
    </Section>
  ));
}

export function NetWorthView({ report, currencies }: ViewProps<NetWorthReport>) {
  const interval = report.range.interval as Interval;
  if (report.currencies.length === 0) return <Empty />;
  return report.currencies.map((c) => (
    <Section key={c.currency} currency={c.currency}>
      <ValueLine
        rows={netWorthRows(c, interval, placesOf(currencies, c.currency))}
        currency={c.currency}
        names={["Net worth", "Change"]}
        label={`Net worth in ${c.currency} at the end of each ${interval}`}
      />
      <ReportTable
        caption={`Net worth in ${c.currency}`}
        firstTitle="Period"
        labels={c.intervals.map((i) => intervalLabel(i.start, interval))}
        columns={[
          { title: "Net worth", values: c.intervals.map((i) => i.value) },
          { title: "Change", values: c.intervals.map((i) => i.change) },
        ]}
        currency={c.currency}
      />
    </Section>
  ));
}

export function BreakdownView({ report, currencies }: ViewProps<BreakdownReport>) {
  if (report.currencies.length === 0) return <Empty />;
  return report.currencies.map((c) => (
    <Section key={c.currency} currency={c.currency}>
      <RankedBars
        rows={breakdownRows(c, placesOf(currencies, c.currency))}
        currency={c.currency}
        names={["Expenses", "Income"]}
        label={`Largest expenses in ${c.currency}`}
      />
      <ReportTable
        caption={`Totals in ${c.currency}`}
        firstTitle="Name"
        labels={c.items.map((i) => i.name ?? "(none)")}
        columns={[
          { title: "Expenses", values: c.items.map((i) => i.expenses) },
          { title: "Income", values: c.items.map((i) => i.income) },
        ]}
        currency={c.currency}
      />
    </Section>
  ));
}

export function BudgetView({ report, currencies }: ViewProps<BudgetReport>) {
  if (report.currencies.length === 0) return <Empty />;
  return report.currencies.map((c) => (
    <Section key={c.currency} currency={c.currency}>
      <PairedBars
        rows={budgetRows(c, placesOf(currencies, c.currency))}
        currency={c.currency}
        names={["Budgeted", "Spent"]}
        label={`Budgeted and spent in ${c.currency} per budget`}
      />
      <ReportTable
        caption={`Budgets in ${c.currency}`}
        firstTitle="Budget"
        labels={c.items.map((i) => i.name ?? "Without budget")}
        columns={[
          { title: "Budgeted", values: c.items.map((i) => i.budgeted) },
          { title: "Spent", values: c.items.map((i) => i.spent) },
        ]}
        currency={c.currency}
      />
    </Section>
  ));
}
