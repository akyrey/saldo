import { fromMinor, toMinor } from "@/api/money";
import { isAfterDay } from "@/lib/dates";
import type { Split, TransactionGroup } from "./queries";

/** Whether a split brings money in, takes it out, or only moves it between own accounts. */
export type Flow = "in" | "out" | "neutral";

export interface CurrencyTotal {
  currency: string;
  /** Signed decimal string: negative means more went out than came in. */
  amount: string;
}

export interface DayGroup {
  /** `YYYY-MM-DD`: each group's latest split date, the order the server lists them in. */
  date: string;
  groups: TransactionGroup[];
  totals: CurrencyTotal[];
}

export interface DaySections {
  upcoming: DayGroup[];
  upcomingTotals: CurrencyTotal[];
  past: DayGroup[];
}

/** On an account's page the direction is relative to it; elsewhere the kind decides. */
export function splitFlow(split: Split, perspectiveAccountId?: string): Flow {
  if (perspectiveAccountId) {
    if (split.source_account_id === perspectiveAccountId) return "out";
    if (split.destination_account_id === perspectiveAccountId) return "in";
  }
  if (split.kind === "withdrawal") return "out";
  if (split.kind === "deposit") return "in";
  return "neutral";
}

/** The split's amount with a minus sign when it goes out. */
export function signedAmount(amount: string, flow: Flow): string {
  return flow === "out" ? `-${amount}` : amount;
}

/** Decimal places of a decimal string, so totals keep the currency's precision. */
function places(amount: string): number {
  return amount.split(".")[1]?.length ?? 0;
}

/** Per-currency sums of `splits`, signed by `flowOf`, in first-seen currency order. */
function sumSplits(splits: readonly Split[], flowOf: (split: Split) => Flow): CurrencyTotal[] {
  const sums = new Map<string, { minor: bigint; places: number }>();
  for (const split of splits) {
    const flow = flowOf(split);
    // The server writes every amount of a currency with the same places; the first one sets them.
    const prev = sums.get(split.currency) ?? { minor: 0n, places: places(split.amount) };
    const minor = toMinor(signedAmount(split.amount, flow), prev.places) ?? 0n;
    sums.set(split.currency, { minor: prev.minor + minor, places: prev.places });
  }
  return [...sums].map(([currency, sum]) => ({
    currency,
    amount: fromMinor(sum.minor, sum.places),
  }));
}

/** Net totals per currency of every non-neutral split, in first-seen currency order. */
function totalsOf(groups: readonly TransactionGroup[], perspective?: string): CurrencyTotal[] {
  const moving = groups
    .flatMap((g) => g.splits)
    .filter((split) => splitFlow(split, perspective) !== "neutral");
  return sumSplits(moving, (split) => splitFlow(split, perspective));
}

/**
 * The total of one group's splits: the net of money in and out, or, for a
 * group that only moves money between own accounts, the amount moved.
 */
export function groupTotals(group: TransactionGroup, perspective?: string): CurrencyTotal[] {
  const net = totalsOf([group], perspective);
  return net.length > 0 ? net : sumSplits(group.splits, () => "neutral");
}

/** A group's latest split date (`YYYY-MM-DD` sorts as text); the server sorts by it. */
export function groupDate(group: TransactionGroup): string | undefined {
  return group.splits.reduce<string | undefined>(
    (latest, split) => (latest === undefined || split.date > latest ? split.date : latest),
    undefined,
  );
}

/** Consecutive groups on the same date, keeping the list's order. */
function byDate(groups: readonly TransactionGroup[], perspective?: string): DayGroup[] {
  const days: { date: string; groups: TransactionGroup[] }[] = [];
  for (const group of groups) {
    const date = groupDate(group);
    if (!date) continue;
    const last = days.at(-1);
    if (last?.date === date) last.groups.push(group);
    else days.push({ date, groups: [group] });
  }
  return days.map((day) => ({ ...day, totals: totalsOf(day.groups, perspective) }));
}

/** Splits the list into upcoming days (after `today`) and the rest, each day with totals. */
export function groupByDay(
  groups: readonly TransactionGroup[],
  today: string,
  perspectiveAccountId?: string,
): DaySections {
  const isUpcoming = (g: TransactionGroup) => isAfterDay(groupDate(g) ?? "", today);
  const upcoming = groups.filter(isUpcoming);
  return {
    upcoming: byDate(upcoming, perspectiveAccountId),
    upcomingTotals: totalsOf(upcoming, perspectiveAccountId),
    past: byDate(
      groups.filter((g) => !isUpcoming(g)),
      perspectiveAccountId,
    ),
  };
}
