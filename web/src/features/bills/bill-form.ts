// The bill form's state and its conversion to API requests. Kept apart from
// the components so it can be tested without rendering.
import type { components } from "@/api/schema";

export type Bill = components["schemas"]["BillView"];
export type BillPeriod = components["schemas"]["BillPeriodView"];
type CreateBillRequest = components["schemas"]["CreateBillRequest"];
type BillPatch = components["schemas"]["BillPatch"];

export const FREQUENCIES = ["weekly", "monthly", "quarterly", "half_year", "yearly"] as const;

/** Every field as the inputs hold it: text. */
export interface BillForm {
  name: string;
  amount_min: string;
  amount_max: string;
  currency: string;
  first_due_date: string;
  end_date: string;
  repeat_freq: string;
  skip: string;
  remind_days_before: string;
  remind_every_days: string;
  active: boolean;
  notes: string;
}

export function emptyBillForm(currency: string, today: string): BillForm {
  return {
    name: "",
    amount_min: "",
    amount_max: "",
    currency,
    first_due_date: today,
    end_date: "",
    repeat_freq: "monthly",
    skip: "0",
    remind_days_before: "3",
    remind_every_days: "1",
    active: true,
    notes: "",
  };
}

export function fromBill(bill: Bill): BillForm {
  return {
    name: bill.name,
    amount_min: bill.amount_min,
    amount_max: bill.amount_max,
    currency: bill.currency,
    first_due_date: bill.first_due_date,
    end_date: bill.end_date ?? "",
    repeat_freq: bill.repeat_freq,
    skip: String(bill.skip),
    remind_days_before: String(bill.remind_days_before),
    remind_every_days: String(bill.remind_every_days),
    active: bill.active,
    notes: bill.notes ?? "",
  };
}

/** One comma becomes the decimal point; the server validates the rest. */
const decimal = (text: string) => text.trim().replace(",", ".");
const count = (text: string) => Number.parseInt(text, 10);
const orNull = (text: string) => (text.trim() === "" ? null : text.trim());

export function toCreateRequest(form: BillForm): CreateBillRequest {
  return {
    name: form.name.trim(),
    amount_min: decimal(form.amount_min),
    amount_max: decimal(form.amount_max),
    currency: form.currency,
    first_due_date: form.first_due_date,
    end_date: orNull(form.end_date),
    repeat_freq: form.repeat_freq,
    skip: count(form.skip),
    remind_days_before: count(form.remind_days_before),
    remind_every_days: count(form.remind_every_days),
    active: form.active,
    notes: orNull(form.notes),
  };
}

/** Only the fields that differ from `bill` (PATCH semantics). */
export function toPatch(bill: Bill, form: BillForm): BillPatch {
  const wanted = toCreateRequest(form);
  const current = toCreateRequest(fromBill(bill));
  const patch: Record<string, unknown> = {};
  for (const key of Object.keys(wanted) as (keyof CreateBillRequest)[]) {
    if (key !== "currency" && wanted[key] !== current[key]) patch[key] = wanted[key];
  }
  return patch as BillPatch;
}

/** `YYYY-MM-DD` moved by `days`, computed in UTC so time zones cannot shift it. */
export function shiftDate(iso: string, days: number): string {
  const date = new Date(`${iso}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

/** "800.00 EUR" or "9.99–12.99 EUR". */
export function amountRange(bill: Bill): string {
  const range =
    bill.amount_min === bill.amount_max ? bill.amount_min : `${bill.amount_min}–${bill.amount_max}`;
  return `${range} ${bill.currency}`;
}
