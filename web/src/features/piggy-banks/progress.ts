// Piggy bank maths for the screen, on exact `bigint` minor units.
import { normalizeAmountInput, toMinor } from "@/api/money";

const FULL = 100n;

/**
 * Whole percent of the target saved (0 to 100, rounded down), or `null`
 * when there is no target or an amount does not parse.
 */
export function progressPercent(
  saved: string,
  target: string | null | undefined,
  places: number,
): number | null {
  if (!target) return null;
  const savedMinor = toMinor(saved, places);
  const targetMinor = toMinor(target, places);
  if (savedMinor === null || targetMinor === null || targetMinor <= 0n) return null;
  const percent = (savedMinor * FULL) / targetMinor;
  const clamped = percent < 0n ? 0n : percent > FULL ? FULL : percent;
  return Number(clamped);
}

export type Direction = "add" | "remove";

/** The signed decimal string the API expects for adding or removing money. */
export function signedAmount(direction: Direction, input: string): string {
  const amount = normalizeAmountInput(input);
  if (direction === "add" || amount === "" || amount.startsWith("-")) return amount;
  return `-${amount}`;
}
