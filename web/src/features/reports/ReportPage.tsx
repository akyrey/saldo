import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { DatePicker } from "@/components/DatePicker";
import { Field } from "@/components/Field";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { currenciesQuery } from "@/features/accounts/queries";
import { todayIso } from "@/lib/dates";
import {
  budgetReportQuery,
  byCategoryQuery,
  byCounterpartyQuery,
  byTagQuery,
  cashFlowQuery,
  categoryAveragesQuery,
  netWorthQuery,
  type ReportParams,
  yearOverYearQuery,
} from "./queries";
import {
  defaultSpan,
  INTERVALS,
  isReportKind,
  REPORTS,
  type ReportKind,
  takesSpan,
} from "./report-model";
import { BreakdownView, BudgetView, CashFlowView, NetWorthView } from "./ReportViews";
import { GoalManager } from "@/features/goals/GoalManager";
import { goalsQuery } from "@/features/goals/queries";
import { TrendControls } from "./TrendControls";
import type { Metric } from "./trend-model";
import { CategoryAveragesView, SavingsRateView, YearOverYearView } from "./TrendViews";

interface TrendParams {
  years: number;
  metric: Metric;
  category_id?: string;
  months: number;
}

/** Year over year and averages: their own parameters, no date span. */
function TrendBody({
  kind,
  trend,
  scope,
}: {
  kind: "year-over-year" | "category-averages";
  trend: TrendParams;
  scope?: string;
}) {
  const currencies = useQuery(currenciesQuery).data;
  const yoy = useQuery({
    ...yearOverYearQuery({ ...trend, scope }),
    enabled: kind === "year-over-year",
  });
  const averages = useQuery({
    ...categoryAveragesQuery(trend.months, scope),
    enabled: kind === "category-averages",
  });
  const active = kind === "year-over-year" ? yoy : averages;
  if (!active.data) return <QueryState isPending={active.isPending} error={active.error} />;
  if (kind === "year-over-year" && yoy.data)
    return <YearOverYearView report={yoy.data} currencies={currencies} />;
  if (averages.data) return <CategoryAveragesView report={averages.data} currencies={currencies} />;
  return null;
}

function ReportBody({ kind, params }: { kind: ReportKind; params: ReportParams }) {
  const currencies = useQuery(currenciesQuery).data;
  const cashFlow = useQuery({
    ...cashFlowQuery(params),
    enabled: kind === "cash-flow" || kind === "savings-rate",
  });
  const netWorth = useQuery({ ...netWorthQuery(params), enabled: kind === "net-worth" });
  const goals = useQuery({ ...goalsQuery, enabled: kind === "net-worth" });
  const byCategory = useQuery({ ...byCategoryQuery(params), enabled: kind === "by-category" });
  const byTag = useQuery({ ...byTagQuery(params), enabled: kind === "by-tag" });
  const byCounterparty = useQuery({
    ...byCounterpartyQuery(params),
    enabled: kind === "by-counterparty",
  });
  const budgets = useQuery({ ...budgetReportQuery(params), enabled: kind === "budget-vs-actual" });
  const active = {
    "cash-flow": cashFlow,
    "savings-rate": cashFlow,
    "net-worth": netWorth,
    "by-category": byCategory,
    "by-counterparty": byCounterparty,
    "by-tag": byTag,
    "budget-vs-actual": budgets,
    // Rendered by TrendBody; never reached here.
    "year-over-year": cashFlow,
    "category-averages": cashFlow,
  }[kind];
  if (!active.data) return <QueryState isPending={active.isPending} error={active.error} />;
  if (kind === "cash-flow" && cashFlow.data)
    return <CashFlowView report={cashFlow.data} currencies={currencies} />;
  if (kind === "savings-rate" && cashFlow.data)
    return <SavingsRateView report={cashFlow.data} currencies={currencies} />;
  if (kind === "by-counterparty" && byCounterparty.data)
    return <BreakdownView report={byCounterparty.data} currencies={currencies} />;
  if (kind === "net-worth" && netWorth.data)
    return (
      <>
        <NetWorthView
          report={netWorth.data}
          currencies={currencies}
          targets={(goals.data ?? [])
            .filter((g) => g.active)
            .map((g) => ({ currency: g.currency, amount: g.target_amount, label: g.name }))}
        />
        <GoalManager />
      </>
    );
  if (kind === "by-category" && byCategory.data)
    return <BreakdownView report={byCategory.data} currencies={currencies} />;
  if (kind === "by-tag" && byTag.data)
    return <BreakdownView report={byTag.data} currencies={currencies} />;
  if (budgets.data) return <BudgetView report={budgets.data} currencies={currencies} />;
  return null;
}

/** One report with its span and interval in the URL. */
export function ReportPage() {
  const { report } = useParams({ from: "/app/reports/$report" });
  const search = useSearch({ from: "/app/reports/$report" });
  const { scope } = useSearch({ from: "/app" });
  const navigate = useNavigate({ from: "/reports/$report" });
  const span = defaultSpan(todayIso());
  const params: ReportParams = {
    from: search.from ?? span.from,
    to: search.to ?? span.to,
    interval: search.interval ?? "month",
    scope,
  };
  const info = REPORTS.find((r) => r.kind === report);
  const trend: TrendParams = {
    years: search.years ?? 3,
    metric: search.metric ?? "expenses",
    category_id: search.category_id,
    months: search.months ?? 12,
  };
  const set = (changes: Partial<Pick<ReportParams, "from" | "to" | "interval">>) =>
    void navigate({ search: (prev) => ({ ...prev, ...changes }) });
  const setTrend = (changes: Partial<TrendParams>) =>
    void navigate({ search: (prev) => ({ ...prev, ...changes }) });
  const kind = isReportKind(report) ? report : null;
  const trendKind = kind === "year-over-year" || kind === "category-averages" ? kind : null;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={info?.title ?? "Report"}
        actions={
          <Button variant="outline" asChild>
            <Link to="/reports">Dashboard</Link>
          </Button>
        }
      />
      <nav aria-label="Reports" className="flex flex-wrap gap-1">
        {REPORTS.map((r) => (
          <Link
            key={r.kind}
            to="/reports/$report"
            params={{ report: r.kind }}
            search={(prev) => prev}
            className="rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent"
            activeProps={{ className: "bg-accent text-foreground font-medium" }}
          >
            {r.title}
          </Link>
        ))}
        <Link
          to="/forecast"
          search={{}}
          className="rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent"
        >
          Forecast
        </Link>
      </nav>
      {trendKind ? <TrendControls kind={trendKind} value={trend} onChange={setTrend} /> : null}
      <div className="grid gap-3 sm:grid-cols-3" hidden={!(kind && takesSpan(kind))}>
        <Field id="report-from" label="From">
          <DatePicker
            id="report-from"
            value={params.from}
            onChange={(value) => value && set({ from: value })}
          />
        </Field>
        <Field id="report-to" label="To">
          <DatePicker
            id="report-to"
            value={params.to}
            onChange={(value) => value && set({ to: value })}
          />
        </Field>
        <Field id="report-interval" label="Interval">
          <NativeSelect
            id="report-interval"
            value={params.interval}
            onChange={(e) => set({ interval: INTERVALS.find((i) => i === e.target.value) })}
          >
            {INTERVALS.map((i) => (
              <option key={i} value={i}>
                {i[0]?.toUpperCase() + i.slice(1)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
      {trendKind ? (
        <TrendBody kind={trendKind} trend={trend} scope={scope} />
      ) : kind ? (
        <ReportBody kind={kind} params={params} />
      ) : (
        <p className="text-sm text-muted-foreground">Unknown report.</p>
      )}
    </div>
  );
}
