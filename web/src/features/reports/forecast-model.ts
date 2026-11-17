// Forecast choices, URL search parsing and chart rows, free of React.
import { isNegative } from "@/api/money";
import type { components } from "@/api/schema";
import { plotValue, type ChartRow } from "./report-model";

export type ForecastCurrency = components["schemas"]["ForecastCurrency"];
export type ForecastItem = components["schemas"]["ForecastItem"];

/** How far ahead the page can look, in months (the server allows up to 24). */
export const HORIZONS = [1, 3, 6, 12, 24] as const;
export type Horizon = (typeof HORIZONS)[number];
export const DEFAULT_HORIZON: Horizon = 3;

export interface ForecastSearch {
  months?: Horizon;
  /** One asset or liability account instead of everything in net worth. */
  account?: string;
}

/** Keeps only a known horizon and an account id from a URL. */
export function parseForecastSearch(search: Record<string, unknown>): ForecastSearch {
  const months = HORIZONS.find((h) => h === Number(search.months));
  const account =
    typeof search.account === "string" && search.account !== "" ? search.account : undefined;
  return { months, account };
}

/** `iso` plus `months` calendar months; the day is clamped to the month's last. */
export function addMonths(iso: string, months: number): string {
  const [year, month, day] = iso.split("-").map(Number) as [number, number, number];
  const total = year * 12 + (month - 1) + months;
  const [y, m] = [Math.floor(total / 12), (total % 12) + 1];
  // Day 0 of the next month is the last day of this one (UTC: no DST shifts).
  const last = new Date(Date.UTC(y, m, 0)).getUTCDate();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${y}-${pad(m)}-${pad(Math.min(day, last))}`;
}

/** The last date and a readable interval for a horizon starting today. */
export function forecastSpan(
  today: string,
  months: number,
): { to: string; interval: "day" | "week" | "month" } {
  const interval = months <= 1 ? "day" : months <= 6 ? "week" : "month";
  return { to: addMonths(today, months), interval };
}

/** One point per interval end; balances may be negative. */
export function forecastRows(currency: ForecastCurrency, places: number): ChartRow[] {
  return currency.points.map((p) => ({
    label: p.date,
    first: plotValue(p.balance, places),
    second: 0,
    firstText: p.balance,
    secondText: "",
  }));
}

export function isBelowZero(balance: string): boolean {
  return isNegative(balance);
}

export interface ItemsOnDate {
  date: string;
  items: ForecastItem[];
}

/** Items (already sorted by date) grouped per date, keeping their order. */
export function groupByDate(items: readonly ForecastItem[]): ItemsOnDate[] {
  return items.reduce<ItemsOnDate[]>((groups, item) => {
    const last = groups.at(-1);
    if (last?.date === item.date) {
      return [...groups.slice(0, -1), { date: last.date, items: [...last.items, item] }];
    }
    return [...groups, { date: item.date, items: [item] }];
  }, []);
}
