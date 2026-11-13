import { Trash2 } from "lucide-react";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { describeRepetition, KINDS, WEEKDAYS, type RepetitionRow } from "./recurrence-model";

const KIND_LABELS: Record<string, string> = {
  daily: "Daily",
  weekly: "Weekly",
  monthly: "Monthly (day of month)",
  nth_weekday: "Monthly (nth weekday)",
  yearly: "Yearly",
};

const WEEKEND_LABELS: Record<string, string> = {
  keep: "Keep weekend dates",
  skip: "Skip weekend dates",
  previous_friday: "Move to the Friday before",
  next_monday: "Move to the Monday after",
};

const MOMENT_HINTS: Record<string, string> = {
  monthly: "Day of the month (1-31; 31 means the last day)",
  nth_weekday: "Week and weekday, e.g. 2,3 for the second Wednesday",
  yearly: "Month and day, e.g. 03-15",
};

interface RepetitionFieldsProps {
  index: number;
  row: RepetitionRow;
  canRemove: boolean;
  onChange: (changes: Partial<Omit<RepetitionRow, "key">>) => void;
  onRemove: () => void;
}

/** One repetition: kind, its moment, how many dates to skip, weekend policy. */
export function RepetitionFields({
  index,
  row,
  canRemove,
  onChange,
  onRemove,
}: RepetitionFieldsProps) {
  const id = (field: string) => `repetition-${index}-${field}`;
  return (
    <fieldset aria-label={`Repetition ${index + 1}`} className="rounded-lg border p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-sm font-medium" data-testid={id("summary")}>
          {describeRepetition({ ...row, skip: Number(row.skip) || 0 })}
        </p>
        {canRemove ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label={`Remove repetition ${index + 1}`}
            onClick={onRemove}
          >
            <Trash2 />
          </Button>
        ) : null}
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Field id={id("kind")} label="Repeats">
          <NativeSelect
            id={id("kind")}
            value={row.kind}
            onChange={(e) => onChange({ kind: e.target.value })}
          >
            {KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {KIND_LABELS[kind]}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {row.kind === "weekly" ? (
          <Field id={id("moment")} label="On">
            <NativeSelect
              id={id("moment")}
              value={row.moment}
              onChange={(e) => onChange({ moment: e.target.value })}
            >
              {WEEKDAYS.map((day, i) => (
                <option key={day} value={String(i + 1)}>
                  {day}
                </option>
              ))}
            </NativeSelect>
          </Field>
        ) : row.kind === "daily" ? null : (
          <Field id={id("moment")} label={MOMENT_HINTS[row.kind] ?? "Moment"}>
            <Input
              id={id("moment")}
              value={row.moment}
              onChange={(e) => onChange({ moment: e.target.value })}
            />
          </Field>
        )}
        <Field id={id("skip")} label="Skip between dates">
          <Input
            id={id("skip")}
            type="number"
            min={0}
            value={row.skip}
            onChange={(e) => onChange({ skip: e.target.value })}
          />
        </Field>
        <Field id={id("weekend")} label="Weekends">
          <NativeSelect
            id={id("weekend")}
            value={row.weekend_policy}
            onChange={(e) => onChange({ weekend_policy: e.target.value })}
          >
            {Object.entries(WEEKEND_LABELS).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </div>
    </fieldset>
  );
}
