// Turns a bill's history (`GET /bills/{id}/history`) into what the payment
// chart draws. Pure functions: the plotted numbers are for drawing only, and
// every label keeps the exact decimal strings.
import type { components } from "@/api/schema";
import { plotValue } from "@/features/reports/report-model";

export type BillHistory = components["schemas"]["BillHistoryView"];
export type HistoryPayment = components["schemas"]["HistoryPayment"];
export type DueStatus = "paid" | "skipped" | "open" | "overdue" | "scheduled";

export const DUE_STATUSES: readonly DueStatus[] = [
  "paid",
  "skipped",
  "open",
  "overdue",
  "scheduled",
];

/** One payment on the chart. */
export interface PaymentPoint {
  /** Milliseconds since the epoch (UTC midnight), for a numeric time axis. */
  x: number;
  y: number;
  date: string;
  description: string;
  amount: string;
  dueDate: string | null;
}

/** One due date on the chart, drawn along the bottom. */
export interface DuePoint {
  x: number;
  y: number;
  date: string;
  status: DueStatus;
}

export interface HistoryChart {
  domain: [number, number];
  ticks: number[];
  /** Years only (long spans) or month and year. */
  tickUnit: "year" | "month";
  today: number | null;
  band: { min: number; max: number };
  /** Payments in the bill's currency, oldest first. */
  payments: PaymentPoint[];
  /** Due dates per status (empty statuses are left out). */
  dues: { status: DueStatus; points: DuePoint[] }[];
}

const MAX_TICKS = 12;
const YEAR_TICKS_AFTER_MONTHS = 36;

/** An ISO date as epoch milliseconds at UTC midnight. */
export function epochOf(iso: string): number {
  const [y = 0, m = 1, d = 1] = iso.split("-").map(Number);
  return Date.UTC(y, m - 1, d);
}

function isDueStatus(value: string): value is DueStatus {
  return DUE_STATUSES.some((s) => s === value);
}

/** Ticks on the first of a month, at most `MAX_TICKS`, inside the domain. */
export function timeTicks(from: string, to: string): { ticks: number[]; unit: "year" | "month" } {
  const [fy = 0, fm = 1] = from.split("-").map(Number);
  const [ty = 0, tm = 1] = to.split("-").map(Number);
  const months = (ty - fy) * 12 + (tm - fm);
  const unit = months > YEAR_TICKS_AFTER_MONTHS ? "year" : "month";
  const base = unit === "year" ? 12 : 1;
  const step = base * Math.max(1, Math.ceil(months / base / MAX_TICKS));
  const [start, end] = [epochOf(from), epochOf(to)];
  const ticks: number[] = [];
  // Start at the first month (or January) on or after `from`.
  let index = fy * 12 + (fm - 1);
  if (unit === "year") index = Math.ceil(index / 12) * 12;
  for (; ; index += step) {
    const tick = Date.UTC(Math.floor(index / 12), index % 12, 1);
    if (tick > end) break;
    if (tick >= start) ticks.push(tick);
  }
  return { ticks, unit };
}

/** The chart for `history`, with amounts plotted at `places` decimals. */
export function historyChart(history: BillHistory, places: number, today: string): HistoryChart {
  const domain: [number, number] = [epochOf(history.range.from), epochOf(history.range.to)];
  const { ticks, unit } = timeTicks(history.range.from, history.range.to);
  const todayX = epochOf(today);
  const payments = history.payments
    .filter((p) => p.currency === history.currency)
    .map((p) => ({
      x: epochOf(p.date),
      y: plotValue(p.amount, places),
      date: p.date,
      description: p.description,
      amount: p.amount,
      dueDate: p.due_date ?? null,
    }))
    .sort((a, b) => a.x - b.x);
  const dues = DUE_STATUSES.map((status) => ({
    status,
    points: history.due_dates
      .filter((d) => isDueStatus(d.status) && d.status === status)
      .map((d) => ({ x: epochOf(d.date), y: 0, date: d.date, status })),
  })).filter((series) => series.points.length > 0);
  return {
    domain,
    ticks,
    tickUnit: unit,
    today: todayX >= domain[0] && todayX <= domain[1] ? todayX : null,
    band: {
      min: plotValue(history.amount_min, places),
      max: plotValue(history.amount_max, places),
    },
    payments,
    dues,
  };
}
