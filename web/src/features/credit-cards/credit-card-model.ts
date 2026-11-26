// Pure helpers for the credit card screens. Amounts stay decimal strings;
// maths goes through bigint minor units (see `@/api/money`).
import { toMinor } from "@/api/money";
import type { Account } from "@/features/accounts/queries";

/** Basis points in one whole (100%). */
const WHOLE_BP = 10_000n;

export interface CardProgress {
  /** Share of the budget spent, 0 to 100, for the bar's width only. */
  percent: number;
  overspent: boolean;
}

/** How much of the month's budget is spent. */
export function cardProgress(spent: string, limit: string, places: number): CardProgress {
  const used = toMinor(spent, places) ?? 0n;
  const total = toMinor(limit, places) ?? 0n;
  if (total <= 0n) return { percent: used > 0n ? 100 : 0, overspent: used > 0n };
  const basisPoints = (used * WHOLE_BP) / total;
  const clamped = basisPoints < 0n ? 0n : basisPoints > WHOLE_BP ? WHOLE_BP : basisPoints;
  return { percent: Number(clamped) / 100, overspent: used > total };
}

const STATE_LABELS: Record<string, string> = {
  synced: "updates until then",
  frozen: "booked",
  detached: "managed by hand",
};

/** What the settlement's state means, in a few words. */
export function settlementStateLabel(state: string): string {
  return STATE_LABELS[state] ?? state;
}

/** "November 2026" for "2026-11". */
export function monthLabel(cycle: string): string {
  const [year, month] = cycle.split("-").map(Number);
  if (!year || !month) return cycle;
  const date = new Date(Date.UTC(year, month - 1, 1));
  return date.toLocaleDateString("en", { month: "long", year: "numeric", timeZone: "UTC" });
}

/** "Oct" for "2026-10": the short name used on moved transactions. */
export function shortMonth(cycle: string): string {
  const [year, month] = cycle.split("-").map(Number);
  if (!year || !month) return cycle;
  const date = new Date(Date.UTC(year, month - 1, 1));
  return date.toLocaleDateString("en", { month: "short", timeZone: "UTC" });
}

/** The month before or after `cycle` ("2026-12" + 1 = "2027-01"). */
export function shiftMonth(cycle: string, by: number): string {
  const [year, month] = cycle.split("-").map(Number);
  if (!year || !month) return cycle;
  const index = year * 12 + (month - 1) + by;
  const next = `${Math.floor(index / 12)}-${String((index % 12) + 1).padStart(2, "0")}`;
  return next.padStart(7, "0");
}

/** Whether a difference left after the payment is worth showing. */
export function hasDifference(difference: string | null | undefined, places: number): boolean {
  if (!difference) return false;
  return (toMinor(difference, places) ?? 0n) !== 0n;
}

/** The month picker's choices: every listed month plus the current one. */
export function budgetMonthChoices(current: string, count = 12): string[] {
  return Array.from({ length: count }, (_, i) => shiftMonth(current, i));
}

/** Accounts that can pay `card`: active asset accounts in its currency. */
export function payingAccounts(accounts: readonly Account[], card: Account): Account[] {
  return accounts.filter(
    (a) => a.type === "asset" && a.active && a.id !== card.id && a.currency === card.currency,
  );
}
