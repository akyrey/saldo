import { CalendarDays } from "lucide-react";
import { Popover } from "radix-ui";
import { lazy, Suspense, useState } from "react";
import { dateParts, parseIsoDate } from "@/lib/dates";
import { cn } from "@/lib/utils";
import { Input } from "./ui/input";

const DateCalendar = lazy(() => import("./DateCalendar"));

interface DatePickerProps {
  id?: string;
  /** `YYYY-MM-DD`, or `""` for no date. */
  value: string;
  /** Called only with a complete, real date or `""`; half-typed text stays local. */
  onChange: (value: string) => void;
  required?: boolean;
  disabled?: boolean;
  className?: string;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
}

/**
 * A date field: type `YYYY-MM-DD`, or open the calendar beside it. Replaces
 * `<input type="date">`, whose picker looks different in every browser.
 */
export function DatePicker({ value, onChange, className, ...props }: DatePickerProps) {
  const [draft, setDraft] = useState<string | null>(null); // null: showing `value`
  const [open, setOpen] = useState(false);
  const label = dateParts(value)?.label;

  const type = (text: string) => {
    const trimmed = text.trim();
    if (trimmed === "" || parseIsoDate(trimmed)) {
      onChange(trimmed);
      setDraft(null);
    } else {
      setDraft(text);
    }
  };
  const pick = (iso: string) => {
    onChange(iso);
    setDraft(null);
    setOpen(false);
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <div className={cn("relative", className)}>
        <Input
          {...props}
          inputMode="numeric"
          placeholder="YYYY-MM-DD"
          title={label}
          value={draft ?? value}
          onChange={(e) => type(e.target.value)}
          // Half-typed text that is not a date goes back to the last real one.
          onBlur={() => setDraft(null)}
          className="pr-9 tabular-nums"
        />
        <Popover.Trigger asChild>
          <button
            type="button"
            disabled={props.disabled}
            aria-label="Choose date"
            className="absolute top-1/2 right-1 -translate-y-1/2 rounded p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <CalendarDays className="size-4" aria-hidden />
          </button>
        </Popover.Trigger>
      </div>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={4}
          className="z-50 rounded-md border bg-background py-2 shadow-md"
        >
          <Suspense fallback={<p className="p-4 text-sm text-muted-foreground">Loading…</p>}>
            <DateCalendar value={value} onPick={pick} clearable={!props.required} />
          </Suspense>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
