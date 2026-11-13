// Budget maths for the screens, free of React. Amounts stay decimal strings
// and are compared as `bigint` minor units, never as floats.
import { normalizeAmountInput, toMinor } from "@/api/money";
import type { BudgetRequest } from "./queries";

/** A calendar month as `YYYY-MM`. */
export type Month = string;

const MONTH = /^(\d{4})-(0[1-9]|1[0-2])$/;

/** The month of an ISO date (`2026-11-13` → `2026-11`). */
export function monthOf(date: string): Month {
  return date.slice(0, 7);
}

/** Accepts `YYYY-MM` from a URL; anything else is `undefined`. */
export function parseMonth(value: unknown): Month | undefined {
  return typeof value === "string" && MONTH.test(value) ? value : undefined;
}

/** First and last day of a month, as ISO dates. */
export function monthRange(month: Month): { from: string; to: string } {
  const [year, number] = month.split("-").map(Number) as [number, number];
  // Day 0 of the next month is the last day of this one (UTC: no DST shifts).
  const last = new Date(Date.UTC(year, number, 0)).getUTCDate();
  return { from: `${month}-01`, to: `${month}-${String(last).padStart(2, "0")}` };
}

/** The month `delta` months away (`2026-12` + 1 → `2027-01`). */
export function shiftMonth(month: Month, delta: number): Month {
  const [year, number] = month.split("-").map(Number) as [number, number];
  const total = year * 12 + (number - 1) + delta;
  return `${Math.floor(total / 12)}-${String((total % 12) + 1).padStart(2, "0")}`;
}

/** A month for headings, e.g. "November 2026". */
export function monthLabel(month: Month, locale?: string): string {
  const [year, number] = month.split("-").map(Number) as [number, number];
  return new Intl.DateTimeFormat(locale, {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(Date.UTC(year, number - 1, 1)));
}

export interface Progress {
  /** 0 to 100, for the bar's width. */
  percent: number;
  overspent: boolean;
}

/** How much of `amount` was `spent`, as a bar. */
export function progress(spent: string, amount: string, places: number): Progress {
  const used = toMinor(spent, places) ?? 0n;
  const total = toMinor(amount, places) ?? 0n;
  if (total <= 0n) return { percent: used > 0n ? 100 : 0, overspent: used > 0n };
  const percent = Number((used * 100n) / total);
  return { percent: Math.max(0, Math.min(100, percent)), overspent: used > total };
}

export const AUTO_TYPES = [
  { value: "reset", label: "Same amount every period" },
  { value: "rollover", label: "Carry what is left over" },
] as const;

export const PERIODS = [
  { value: "daily", label: "Day" },
  { value: "weekly", label: "Week" },
  { value: "monthly", label: "Month" },
  { value: "quarterly", label: "Quarter" },
  { value: "half_year", label: "Half year" },
  { value: "yearly", label: "Year" },
] as const;

/** What the budget form holds: every input is a string. */
export interface BudgetForm {
  name: string;
  active: boolean;
  notes: string;
  /** `""` for no automatic limit. */
  autoType: string;
  autoPeriod: string;
  autoAmount: string;
}

export const EMPTY_BUDGET_FORM: BudgetForm = {
  name: "",
  active: true,
  notes: "",
  autoType: "",
  autoPeriod: "monthly",
  autoAmount: "",
};

/** The request body for creating or replacing a budget's fields. */
export function toBudgetRequest(form: BudgetForm): BudgetRequest {
  const notes = form.notes.trim();
  return {
    name: form.name.trim(),
    active: form.active,
    notes: notes === "" ? null : notes,
    auto_budget:
      form.autoType === ""
        ? null
        : {
            type: form.autoType,
            period: form.autoPeriod,
            amount: normalizeAmountInput(form.autoAmount),
          },
  };
}
