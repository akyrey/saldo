// Report choices, URL search parsing and chart rows, free of React.
import { toMinor } from "@/api/money";
import type { components } from "@/api/schema";
import { monthOf, shiftMonth } from "@/features/budgets/budget-model";

export type Interval = "day" | "week" | "month" | "year";
export const INTERVALS: readonly Interval[] = ["day", "week", "month", "year"];

export type ReportKind =
  | "cash-flow"
  | "savings-rate"
  | "net-worth"
  | "by-category"
  | "by-counterparty"
  | "category-averages"
  | "year-over-year"
  | "by-tag"
  | "budget-vs-actual";

export const REPORTS: readonly { kind: ReportKind; title: string; description: string }[] = [
  { kind: "cash-flow", title: "Cash flow", description: "Income and expenses" },
  { kind: "savings-rate", title: "Savings rate", description: "The share of income kept" },
  { kind: "net-worth", title: "Net worth", description: "Assets minus debts over time" },
  { kind: "by-category", title: "By category", description: "Where the money went" },
  { kind: "by-counterparty", title: "By payee", description: "Who the money went to" },
  {
    kind: "category-averages",
    title: "Averages",
    description: "A normal month per category",
  },
  { kind: "year-over-year", title: "Year over year", description: "This year against last" },
  { kind: "by-tag", title: "By tag", description: "Spending per tag" },
  { kind: "budget-vs-actual", title: "Budgets", description: "Budgeted against spent" },
];

/** Reports that take a date span and an interval (the others have their own controls). */
export function takesSpan(kind: ReportKind): boolean {
  return kind !== "year-over-year" && kind !== "category-averages";
}

export function isReportKind(value: string): value is ReportKind {
  return REPORTS.some((r) => r.kind === value);
}

export interface ReportSearch {
  from?: string;
  to?: string;
  interval?: Interval;
}

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

/** Keeps only valid `from`, `to` (ISO dates) and `interval` from a URL. */
export function parseReportSearch(search: Record<string, unknown>): ReportSearch {
  const date = (value: unknown) =>
    typeof value === "string" && ISO_DATE.test(value) ? value : undefined;
  const interval = INTERVALS.find((i) => i === search.interval);
  return { from: date(search.from), to: date(search.to), interval };
}

/** The span shown when the URL names none: the twelve months up to `today`. */
export function defaultSpan(today: string): { from: string; to: string } {
  return { from: `${shiftMonth(monthOf(today), -11)}-01`, to: today };
}

/**
 * A decimal string as a JS number, **for drawing only**: chart libraries
 * need numbers. Totals and labels always use the exact strings.
 */
export function plotValue(amount: string, places: number): number {
  const minor = toMinor(amount, places);
  return minor === null ? 0 : Number(minor) / 10 ** places;
}

type CashFlowCurrency = components["schemas"]["CashFlowCurrency"];
type NetWorthCurrency = components["schemas"]["NetWorthCurrency"];
type BreakdownCurrency = components["schemas"]["BreakdownCurrency"];
type BudgetCurrency = components["schemas"]["BudgetReportCurrency"];

/** One bar or point of a chart: a label, two plotted values and their exact text. */
export interface ChartRow {
  label: string;
  first: number;
  second: number;
  firstText: string;
  secondText: string;
}

/** A short label for an interval starting at `start`. */
export function intervalLabel(start: string, interval: Interval): string {
  if (interval === "year") return start.slice(0, 4);
  if (interval === "month") return start.slice(0, 7);
  return start;
}

export function cashFlowRows(c: CashFlowCurrency, interval: Interval, places: number): ChartRow[] {
  return c.intervals.map((i) => ({
    label: intervalLabel(i.start, interval),
    first: plotValue(i.income, places),
    second: plotValue(i.expenses, places),
    firstText: i.income,
    secondText: i.expenses,
  }));
}

export function netWorthRows(c: NetWorthCurrency, interval: Interval, places: number): ChartRow[] {
  return c.intervals.map((i) => ({
    label: intervalLabel(i.start, interval),
    first: plotValue(i.value, places),
    second: plotValue(i.change, places),
    firstText: i.value,
    secondText: i.change,
  }));
}

/** The largest `limit` expense items (no zero rows); the rest are left to the table. */
export function breakdownRows(c: BreakdownCurrency, places: number, limit = 10): ChartRow[] {
  return c.items
    .filter((item) => (toMinor(item.expenses, places) ?? 0n) > 0n)
    .slice(0, limit)
    .map((item) => ({
      label: item.name ?? "(none)",
      first: plotValue(item.expenses, places),
      second: plotValue(item.income, places),
      firstText: item.expenses,
      secondText: item.income,
    }));
}

export function budgetRows(c: BudgetCurrency, places: number): ChartRow[] {
  return c.items.map((item) => ({
    label: item.name ?? "Without budget",
    first: plotValue(item.budgeted, places),
    second: plotValue(item.spent, places),
    firstText: item.budgeted,
    secondText: item.spent,
  }));
}
