import { dateParts } from "@/lib/dates";
import { cn } from "@/lib/utils";

interface DateTileProps {
  /** A `YYYY-MM-DD` calendar date. */
  date: string;
  /** Today as `YYYY-MM-DD`; the year is shown only for other years. */
  today: string;
  className?: string;
}

/** A small calendar-page rendering of a date: month on top, day large, weekday below. */
export function DateTile({ date, today, className }: DateTileProps) {
  const parts = dateParts(date);
  if (!parts) return <span className={className}>{date}</span>;
  const otherYear = parts.year !== today.slice(0, 4);
  return (
    <time
      dateTime={date}
      title={parts.label}
      aria-label={parts.label}
      className={cn(
        "inline-flex w-11 flex-col items-center overflow-hidden rounded-md border bg-card text-center leading-none",
        className,
      )}
    >
      <span className="w-full bg-muted py-0.5 text-[0.625rem] font-medium uppercase tracking-wide text-muted-foreground">
        {parts.month}
        {otherYear ? ` ’${parts.year.slice(2)}` : null}
      </span>
      <span className="pt-1 text-base font-semibold tabular-nums">{parts.day}</span>
      <span className="pb-1 text-[0.625rem] text-muted-foreground">{parts.weekday}</span>
    </time>
  );
}
