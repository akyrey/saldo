// Loaded on first open (see DatePicker), so react-day-picker and date-fns
// stay out of the page bundles until someone opens a calendar.
import { DayPicker } from "react-day-picker";
import "react-day-picker/style.css";
import { addDays, parseIsoDate, todayIso } from "@/lib/dates";
import { Button } from "./ui/button";

/** How far back and ahead the month and year dropdowns reach. */
const FIRST_YEAR = 1990;
const YEARS_AHEAD = 10;

export interface DateCalendarProps {
  value: string;
  onPick: (iso: string) => void;
  /** Optional fields get a "Clear" button. */
  clearable: boolean;
}

/** A month view with month/year dropdowns and quick picks for today and yesterday. */
export default function DateCalendar({ value, onPick, clearable }: DateCalendarProps) {
  const today = todayIso();
  const selected = parseIsoDate(value) ?? undefined;
  const now = new Date();
  return (
    <div className="saldo-calendar">
      <DayPicker
        mode="single"
        selected={selected}
        defaultMonth={selected ?? now}
        onSelect={(date) => date && onPick(todayIso(date))}
        captionLayout="dropdown"
        startMonth={new Date(FIRST_YEAR, 0)}
        endMonth={new Date(now.getFullYear() + YEARS_AHEAD, 11)}
        weekStartsOn={1}
        showOutsideDays
        autoFocus
      />
      <div className="flex gap-2 border-t px-3 pt-2">
        <Button type="button" size="sm" variant="secondary" onClick={() => onPick(today)}>
          Today
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={() => onPick(addDays(today, -1))}>
          Yesterday
        </Button>
        {clearable ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="ml-auto"
            onClick={() => onPick("")}
          >
            Clear
          </Button>
        ) : null}
      </div>
    </div>
  );
}
