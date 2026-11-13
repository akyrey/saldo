// The recurrence editor's state and rules, free of React. Every function
// returns a new state; nothing is mutated.
import { normalizeAmountInput } from "@/api/money";
import { emptySplit, type SplitRow } from "@/features/transactions/editor-model";
import type { PreviewRequest, Recurrence, RecurrenceRequest, Repetition } from "./queries";

export type { Recurrence };

export const KINDS = ["daily", "weekly", "monthly", "nth_weekday", "yearly"] as const;
export const WEEKEND_POLICIES = ["keep", "skip", "previous_friday", "next_monday"] as const;
export const WEEKDAYS = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
] as const;
const ORDINALS = ["first", "second", "third", "fourth", "fifth"] as const;
const PREVIEW_COUNT = 5;

export interface RepetitionRow {
  key: string;
  kind: string;
  moment: string;
  skip: string;
  weekend_policy: string;
}

/** A transaction split row plus the piggy bank a transfer feeds. */
export interface TemplateRow extends SplitRow {
  piggy_bank_id: string;
}

export interface RecurrenceState {
  title: string;
  first_date: string;
  repeat_until: string;
  max_occurrences: string;
  active: boolean;
  apply_rules: boolean;
  notes: string;
  repetitions: RepetitionRow[];
  splits: TemplateRow[];
}

let nextKey = 0;
function newKey(): string {
  nextKey += 1;
  return `repetition-${nextKey}`;
}

/** Weekday 1 (Monday) to 7 (Sunday), day, month and week-of-month of a date. */
function partsOf(iso: string) {
  const date = new Date(`${iso}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return undefined;
  const day = date.getUTCDate();
  return {
    weekday: ((date.getUTCDay() + 6) % 7) + 1,
    day,
    month: date.getUTCMonth() + 1,
    week: Math.ceil(day / 7),
  };
}

const pad = (n: number) => String(n).padStart(2, "0");

/** A sensible moment for `kind`, taken from the first date. */
export function defaultMoment(kind: string, firstDate: string): string {
  const parts = partsOf(firstDate) ?? { weekday: 1, day: 1, month: 1, week: 1 };
  switch (kind) {
    case "weekly":
      return String(parts.weekday);
    case "monthly":
      return String(parts.day);
    case "nth_weekday":
      return `${parts.week},${parts.weekday}`;
    case "yearly":
      return `${pad(parts.month)}-${pad(parts.day)}`;
    default:
      return "";
  }
}

export function emptyRepetition(firstDate: string, kind = "monthly"): RepetitionRow {
  return {
    key: newKey(),
    kind,
    moment: defaultMoment(kind, firstDate),
    skip: "0",
    weekend_policy: "keep",
  };
}

export function emptyTemplate(): TemplateRow {
  return { ...emptySplit(""), piggy_bank_id: "" };
}

export function emptyRecurrence(today: string): RecurrenceState {
  return {
    title: "",
    first_date: today,
    repeat_until: "",
    max_occurrences: "",
    active: true,
    apply_rules: true,
    notes: "",
    repetitions: [emptyRepetition(today)],
    splits: [emptyTemplate()],
  };
}

/** Changes one repetition; a new kind gets a fresh moment. */
export function updateRepetition(
  state: RecurrenceState,
  index: number,
  changes: Partial<Omit<RepetitionRow, "key">>,
): RecurrenceState {
  return {
    ...state,
    repetitions: state.repetitions.map((row, i) => {
      if (i !== index) return row;
      const kindChanged = changes.kind !== undefined && changes.kind !== row.kind;
      const moment = kindChanged
        ? defaultMoment(changes.kind ?? row.kind, state.first_date)
        : (changes.moment ?? row.moment);
      return { ...row, ...changes, moment };
    }),
  };
}

export function fromRecurrence(r: Recurrence): RecurrenceState {
  return {
    title: r.title,
    first_date: r.first_date,
    repeat_until: r.repeat_until ?? "",
    max_occurrences:
      r.max_occurrences === null || r.max_occurrences === undefined
        ? ""
        : String(r.max_occurrences),
    active: r.active,
    apply_rules: r.apply_rules,
    notes: r.notes ?? "",
    repetitions: r.repetitions.map((rep) => ({
      key: newKey(),
      kind: rep.kind,
      moment: rep.moment,
      skip: String(rep.skip),
      weekend_policy: rep.weekend_policy,
    })),
    splits: r.splits.map((s) => ({
      ...emptyTemplate(),
      description: s.description,
      source_id: s.source_account_id,
      destination_id: s.destination_account_id,
      amount: s.amount,
      foreign_amount: s.foreign_amount ?? "",
      foreign_currency: s.foreign_currency ?? "",
      category_id: s.category_id ?? "",
      tag_ids: [...s.tag_ids],
      piggy_bank_id: s.piggy_bank_id ?? "",
    })),
  };
}

function orNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** A whole number from a text field; `null` when empty. */
function wholeOrNull(value: string): number | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : Number.parseInt(trimmed, 10);
}

function repetitionsOf(state: RecurrenceState) {
  return state.repetitions.map((r) => ({
    kind: r.kind,
    moment: r.moment.trim(),
    skip: wholeOrNull(r.skip) ?? 0,
    weekend_policy: r.weekend_policy,
  }));
}

export function toRequest(state: RecurrenceState): RecurrenceRequest {
  return {
    title: state.title.trim(),
    first_date: state.first_date,
    repeat_until: orNull(state.repeat_until),
    max_occurrences: wholeOrNull(state.max_occurrences),
    active: state.active,
    apply_rules: state.apply_rules,
    notes: orNull(state.notes),
    repetitions: repetitionsOf(state),
    splits: state.splits.map((row) => {
      const foreignAmount = orNull(row.foreign_amount);
      return {
        description: row.description.trim(),
        source_id: row.source_id,
        destination_id: row.destination_id,
        amount: normalizeAmountInput(row.amount),
        foreign_amount: foreignAmount === null ? null : normalizeAmountInput(foreignAmount),
        foreign_currency: orNull(row.foreign_currency),
        category_id: orNull(row.category_id),
        piggy_bank_id: orNull(row.piggy_bank_id),
        tag_ids: row.tag_ids,
      };
    }),
  };
}

/** The preview request, or `null` while the schedule is incomplete. */
export function toPreviewRequest(state: RecurrenceState): PreviewRequest | null {
  if (state.first_date === "" || state.repetitions.length === 0) return null;
  return {
    first_date: state.first_date,
    repeat_until: orNull(state.repeat_until),
    max_occurrences: wholeOrNull(state.max_occurrences),
    repetitions: repetitionsOf(state),
    count: PREVIEW_COUNT,
  };
}

const WEEKEND_TEXT: Record<string, string> = {
  skip: ", not on weekends",
  previous_friday: ", weekends move to Friday",
  next_monday: ", weekends move to Monday",
};

function every(skip: number, one: string, many: string): string {
  return skip === 0 ? one : `Every ${skip + 1} ${many}`;
}

function weekdayName(text: string | undefined): string {
  return WEEKDAYS[Number(text) - 1] ?? "?";
}

/** A repetition as a sentence, e.g. "Every second Wednesday of the month". */
export function describeRepetition(
  r: Pick<Repetition, "kind" | "moment" | "skip" | "weekend_policy">,
): string {
  const skip = Number(r.skip) || 0;
  let text: string;
  switch (r.kind) {
    case "daily":
      text = every(skip, "Every day", "days");
      break;
    case "weekly":
      text =
        skip === 0
          ? `Every ${weekdayName(r.moment)}`
          : `Every ${skip + 1} weeks on ${weekdayName(r.moment)}`;
      break;
    case "monthly":
      text = `${every(skip, "Monthly", "months")} on day ${r.moment}`;
      break;
    case "nth_weekday": {
      const [week, day] = r.moment.split(",");
      const ordinal = ORDINALS[Number(week) - 1] ?? "?";
      text = `${every(skip, "Every", "months, every")} ${ordinal} ${weekdayName(day)} of the month`;
      break;
    }
    case "yearly":
      text = `${every(skip, "Every year", "years")} on ${r.moment}`;
      break;
    default:
      text = `${r.kind} ${r.moment}`;
  }
  return text + (WEEKEND_TEXT[r.weekend_policy] ?? "");
}
