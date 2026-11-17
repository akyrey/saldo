import { useQuery } from "@tanstack/react-query";
import { Link, useSearch } from "@tanstack/react-router";
import { Amount } from "@/components/Amount";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { currenciesQuery, placesOf } from "@/features/accounts/queries";
import { monthOf, monthRange } from "@/features/budgets/budget-model";
import { todayIso } from "@/lib/dates";
import { PairedBars, ValueLine } from "./charts";
import { candidatesQuery } from "@/features/subscriptions/queries";
import { forecastSpan, isBelowZero } from "./forecast-model";
import { byCategoryQuery, cashFlowQuery, forecastQuery, netWorthQuery } from "./queries";
import { cashFlowRows, defaultSpan, netWorthRows, REPORTS } from "./report-model";

/** An overview: net worth and cash flow over a year, this month's top categories. */
export function DashboardPage() {
  const { scope } = useSearch({ from: "/app" });
  const today = todayIso();
  const year = { ...defaultSpan(today), interval: "month" as const, scope };
  const month = { ...monthRange(monthOf(today)), interval: "month" as const, scope };
  const currencies = useQuery(currenciesQuery).data;
  const netWorth = useQuery(netWorthQuery(year));
  const cashFlow = useQuery(cashFlowQuery(year));
  const categories = useQuery(byCategoryQuery(month));
  const forecast = useQuery(forecastQuery({ ...forecastSpan(today, 1), scope }));
  const nextMonth = forecast.data?.currencies[0];
  const suggestions = useQuery(candidatesQuery(scope)).data?.length ?? 0;
  const primaryWorth = netWorth.data?.currencies[0];
  const primaryFlow = cashFlow.data?.currencies[0];
  const topCategories = categories.data?.currencies[0];

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Dashboard" />
      {suggestions > 0 ? (
        <p className="rounded-md border px-3 py-2 text-sm" data-testid="subscription-hint">
          {suggestions === 1
            ? "1 repeating payment has no bill yet."
            : `${suggestions} repeating payments have no bill yet.`}{" "}
          <Link to="/bills" className="text-primary underline-offset-4 hover:underline">
            Review on the bills page
          </Link>
        </p>
      ) : null}
      <nav aria-label="Reports" className="flex flex-wrap gap-2">
        {REPORTS.map((r) => (
          <Link
            key={r.kind}
            to="/reports/$report"
            params={{ report: r.kind }}
            search={{}}
            className="rounded-md border px-3 py-1.5 text-sm hover:bg-accent"
          >
            {r.title}
          </Link>
        ))}
        <Link
          to="/forecast"
          search={{}}
          className="rounded-md border px-3 py-1.5 text-sm hover:bg-accent"
        >
          Forecast
        </Link>
      </nav>
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Net worth</CardTitle>
          </CardHeader>
          <CardContent>
            <QueryState isPending={netWorth.isPending} error={netWorth.error} />
            {primaryWorth ? (
              <>
                <p className="mb-2 text-2xl font-semibold" data-testid="net-worth-now">
                  <Amount
                    value={primaryWorth.intervals.at(-1)?.value ?? primaryWorth.opening}
                    currency={primaryWorth.currency}
                  />
                </p>
                <ValueLine
                  rows={netWorthRows(
                    primaryWorth,
                    "month",
                    placesOf(currencies, primaryWorth.currency),
                  )}
                  currency={primaryWorth.currency}
                  names={["Net worth", "Change"]}
                  label="Net worth at the end of each month"
                />
              </>
            ) : null}
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Cash flow</CardTitle>
          </CardHeader>
          <CardContent>
            <QueryState isPending={cashFlow.isPending} error={cashFlow.error} />
            {primaryFlow ? (
              <PairedBars
                rows={cashFlowRows(
                  primaryFlow,
                  "month",
                  placesOf(currencies, primaryFlow.currency),
                )}
                currency={primaryFlow.currency}
                names={["Income", "Expenses"]}
                label="Income and expenses per month"
              />
            ) : cashFlow.data ? (
              <p className="text-sm text-muted-foreground">No income or expenses yet.</p>
            ) : null}
          </CardContent>
        </Card>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>Next 30 days</CardTitle>
        </CardHeader>
        <CardContent>
          <QueryState isPending={forecast.isPending} error={forecast.error} />
          {nextMonth ? (
            <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1 text-sm">
              <span>
                In 30 days:{" "}
                <Amount
                  value={nextMonth.points.at(-1)?.balance ?? nextMonth.start_balance}
                  currency={nextMonth.currency}
                  className="text-lg font-semibold"
                />
              </span>
              <span
                data-testid="next-30-lowest"
                className={isBelowZero(nextMonth.lowest.balance) ? "text-destructive" : undefined}
              >
                Lowest: <Amount value={nextMonth.lowest.balance} currency={nextMonth.currency} /> on{" "}
                {nextMonth.lowest.date}
              </span>
              <span className="text-muted-foreground">{nextMonth.items.length} planned</span>
              <Link
                to="/forecast"
                search={{}}
                className="text-primary underline-offset-4 hover:underline"
              >
                Open forecast
              </Link>
            </div>
          ) : null}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Spending this month by category</CardTitle>
        </CardHeader>
        <CardContent>
          <QueryState isPending={categories.isPending} error={categories.error} />
          {topCategories ? (
            <ul className="divide-y text-sm">
              {topCategories.items.slice(0, 8).map((item) => (
                <li key={item.id ?? "none"} className="flex justify-between gap-2 py-2">
                  <span>{item.name ?? "(no category)"}</span>
                  <Amount value={item.expenses} currency={topCategories.currency} />
                </li>
              ))}
            </ul>
          ) : categories.data ? (
            <p className="text-sm text-muted-foreground">Nothing spent this month.</p>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}
