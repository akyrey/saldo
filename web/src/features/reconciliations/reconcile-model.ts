// Reconciliation maths for the screens, free of React. Amounts are decimal
// strings compared as `bigint` minor units, never as floats.
import { fromMinor, normalizeAmountInput, toMinor } from "@/api/money";
import type { DraftSplit } from "./queries";

export interface Tally {
  /** The cleared balance plus the selected splits. */
  cleared: string;
  /** Statement minus cleared; `null` while the statement balance is not valid. */
  difference: string | null;
  balanced: boolean;
}

export function tally(
  clearedBalance: string,
  splits: readonly DraftSplit[],
  selected: ReadonlySet<string>,
  statementBalance: string,
  places: number,
): Tally {
  const cleared = splits
    .filter((s) => selected.has(s.id))
    .reduce(
      (sum, s) => sum + (toMinor(s.amount, places) ?? 0n),
      toMinor(clearedBalance, places) ?? 0n,
    );
  const statement =
    statementBalance.trim() === "" ? null : toMinor(normalizeAmountInput(statementBalance), places);
  if (statement === null) {
    return { cleared: fromMinor(cleared, places), difference: null, balanced: false };
  }
  const difference = statement - cleared;
  return {
    cleared: fromMinor(cleared, places),
    difference: fromMinor(difference, places),
    balanced: difference === 0n,
  };
}

export interface ReconcileSearch {
  /** The statement date, `YYYY-MM-DD`. */
  date?: string;
  /** The statement's closing balance (from an imported statement). */
  balance?: string;
}

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

export function parseReconcileSearch(search: Record<string, unknown>): ReconcileSearch {
  const date =
    typeof search.date === "string" && ISO_DATE.test(search.date) ? search.date : undefined;
  // The router reads `balance=12.50` from a URL as the number 12.5.
  const balance =
    typeof search.balance === "string"
      ? search.balance
      : typeof search.balance === "number" && Number.isFinite(search.balance)
        ? String(search.balance)
        : undefined;
  return { date, balance };
}
