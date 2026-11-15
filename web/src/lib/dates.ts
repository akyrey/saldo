/** Today's date in the browser's time zone, as `YYYY-MM-DD`. */
export function todayIso(now: Date = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

/** A `YYYY-MM-DD` calendar date as local midnight, or null if it is not a real date. */
export function parseIsoDate(iso: string): Date | null {
  const match = ISO_DATE.exec(iso);
  if (!match) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const date = new Date(year, month - 1, day);
  // `new Date` rolls 30 February over into March; reject that instead.
  if (date.getMonth() !== month - 1 || date.getDate() !== day) return null;
  return date;
}

/** Whether calendar date `a` is after `b` (both `YYYY-MM-DD`, which sort as text). */
export function isAfterDay(a: string, b: string): boolean {
  return a > b;
}

export interface DateParts {
  weekday: string;
  day: string;
  month: string;
  year: string;
  /** The full date for a tooltip or screen reader, e.g. "Sunday, 15 November 2026". */
  label: string;
}

/** Pieces of a calendar date for display; `locale` defaults to the browser's. */
export function dateParts(iso: string, locale?: string): DateParts | null {
  const date = parseIsoDate(iso);
  if (!date) return null;
  const format = (options: Intl.DateTimeFormatOptions) =>
    new Intl.DateTimeFormat(locale, options).format(date);
  return {
    weekday: format({ weekday: "short" }),
    day: String(date.getDate()),
    month: format({ month: "short" }),
    year: String(date.getFullYear()),
    label: format({ weekday: "long", day: "numeric", month: "long", year: "numeric" }),
  };
}

/** Adds whole days to a `YYYY-MM-DD` date (negative to go back); invalid input stays as is. */
export function addDays(iso: string, days: number): string {
  const date = parseIsoDate(iso);
  if (!date) return iso;
  return todayIso(new Date(date.getFullYear(), date.getMonth(), date.getDate() + days));
}

/** "Today", "Yesterday" or "Tomorrow" relative to `today`, otherwise null. */
export function relativeDayName(iso: string, today: string): string | null {
  if (iso === today) return "Today";
  if (iso === addDays(today, -1)) return "Yesterday";
  if (iso === addDays(today, 1)) return "Tomorrow";
  return null;
}

/** "Today", "Yesterday", "Tomorrow", otherwise the full weekday ("Saturday"). */
export function dayName(iso: string, today: string, locale?: string): string {
  const relative = relativeDayName(iso, today);
  const date = parseIsoDate(iso);
  if (relative || !date) return relative ?? iso;
  return new Intl.DateTimeFormat(locale, { weekday: "long" }).format(date);
}

/**
 * A heading for a day in a list: "Today", "Yesterday", "Tomorrow", otherwise
 * weekday, day and month ("Sat, 14 Nov"), plus the year when it is not this year's.
 */
export function dayHeading(iso: string, today: string, locale?: string): string {
  const relative = relativeDayName(iso, today);
  if (relative) return relative;
  const date = parseIsoDate(iso);
  if (!date) return iso;
  const sameYear = iso.slice(0, 4) === today.slice(0, 4);
  return new Intl.DateTimeFormat(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  }).format(date);
}
