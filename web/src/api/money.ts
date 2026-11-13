// Money in the browser. Amounts travel as decimal strings ("12.50") and are
// never turned into JS numbers, which are floats (0.1 + 0.2 !== 0.3). For
// arithmetic we use `bigint` minor units, the same idea as the server's
// `i64` cents: like storing cents in an `int` column in Laravel or Go.

const DECIMAL = /^(-?)(\d+)(?:\.(\d+))?$/;

/** Parses a strict decimal string into minor units, or `null` if invalid or too precise. */
export function toMinor(amount: string, places: number): bigint | null {
  const match = DECIMAL.exec(amount);
  if (!match) return null;
  const [, sign = "", whole = "", fraction = ""] = match;
  if (fraction.length > places) return null;
  const minor = BigInt(whole + fraction.padEnd(places, "0"));
  return sign === "-" ? -minor : minor;
}

/** Turns minor units back into a decimal string with exactly `places` decimals. */
export function fromMinor(minor: bigint, places: number): string {
  const negative = minor < 0n;
  const digits = (negative ? -minor : minor).toString().padStart(places + 1, "0");
  const whole = digits.slice(0, digits.length - places);
  const fraction = places > 0 ? "." + digits.slice(digits.length - places) : "";
  return (negative ? "-" : "") + whole + fraction;
}

/**
 * Cleans what a person typed into the strict form the server accepts:
 * trims, drops spaces, and reads a single comma as the decimal separator
 * ("12,5" → "12.5"). Does not validate; use `toMinor` for that.
 */
export function normalizeAmountInput(text: string): string {
  const compact = text.trim().replace(/\s+/g, "");
  if (!compact.includes(".") && (compact.match(/,/g) ?? []).length === 1) {
    return compact.replace(",", ".");
  }
  return compact;
}

/** Sums decimal strings exactly; invalid entries are skipped. */
export function sumAmounts(amounts: readonly string[], places: number): string {
  let total = 0n;
  for (const amount of amounts) total += toMinor(amount, places) ?? 0n;
  return fromMinor(total, places);
}

/** True for a negative decimal string such as "-3.00". */
export function isNegative(amount: string): boolean {
  return amount.trim().startsWith("-");
}

/**
 * Formats a decimal string for display with the currency symbol, using the
 * string form of `Intl.NumberFormat.format`, which keeps every digit exact.
 */
export function formatMoney(amount: string, currency: string, locale?: string): string {
  const fraction = DECIMAL.exec(amount)?.[3]?.length ?? 0;
  try {
    const format = new Intl.NumberFormat(locale, {
      style: "currency",
      currency,
      minimumFractionDigits: fraction,
      maximumFractionDigits: fraction,
    });
    return format.format(amount as Intl.StringNumericLiteral);
  } catch {
    // Unknown currency codes (crypto, custom) fall back to the plain string.
    return `${amount} ${currency}`;
  }
}
