// Year over year, savings rate and averages: parsing and shaping, free of React.
import type { components } from "@/api/schema";
import { plotValue } from "./report-model";

export type YearOverYearReport = components["schemas"]["YearOverYearReport"];
export type CategoryAveragesReport = components["schemas"]["CategoryAveragesReport"];

export type Metric = "expenses" | "income" | "savings";
export const METRICS: readonly { value: Metric; label: string }[] = [
  { value: "expenses", label: "Spending" },
  { value: "income", label: "Income" },
  { value: "savings", label: "Savings" },
];
export const YEAR_CHOICES = [2, 3, 4, 5] as const;
export const MONTH_CHOICES = [3, 6, 12, 24] as const;
export const MONTH_NAMES = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
] as const;

export interface TrendSearch {
  years?: number;
  metric?: Metric;
  /** A category ID, or `none` for spending without one. */
  category_id?: string;
  months?: number;
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function pickNumber(value: unknown, allowed: readonly number[]): number | undefined {
  const n = Number(value);
  return allowed.includes(n) ? n : undefined;
}

/** Keeps only valid year-over-year and averages parameters from a URL. */
export function parseTrendSearch(search: Record<string, unknown>): TrendSearch {
  const category = search.category_id;
  const parsed: TrendSearch = {
    years: pickNumber(search.years, YEAR_CHOICES),
    metric: METRICS.find((m) => m.value === search.metric)?.value,
    category_id:
      category === "none" || (typeof category === "string" && UUID.test(category))
        ? category
        : undefined,
    months: pickNumber(search.months, MONTH_CHOICES),
  };
  return Object.fromEntries(
    Object.entries(parsed).filter(([, v]) => v !== undefined),
  ) as TrendSearch;
}

/**
 * Basis points (1/100 of a percent) as text, with integer maths so 2500
 * is exactly "25.00%". `—` when there is no figure.
 */
export function formatBp(bp: number | null | undefined): string {
  if (bp === null || bp === undefined) return "—";
  const sign = bp < 0 ? "-" : "";
  const abs = Math.abs(bp);
  return `${sign}${Math.trunc(abs / 100)}.${String(abs % 100).padStart(2, "0")}%`;
}

/** One chart row per month: `{ label, <year>: value | null }`. */
export type YearRow = { label: string } & Record<number, number | null>;

export function yearRows(
  years: readonly { year: number; months: readonly (string | null | undefined)[] }[],
  places: number,
): YearRow[] {
  return MONTH_NAMES.map((label, month) => {
    const row: YearRow = { label };
    for (const y of years) {
      const value = y.months[month];
      row[y.year] = value === null || value === undefined ? null : plotValue(value, places);
    }
    return row;
  });
}
