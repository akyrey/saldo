// Desktop only: bill reminders from the live stream become system
// notifications. Browsers and phones get the same reminders by Web Push.
import { api, call } from "@/api/client";
import { platform } from "@/platform";
import type { EventNotice } from "./live-events";

const TITLES: Record<string, string> = {
  "bill.due_soon": "Bill due soon",
  "bill.overdue": "Bill overdue",
};

interface BillSummary {
  name: string;
  amount_min: string;
  amount_max: string;
  currency: string;
  next_due_date?: string | null;
}

/** `800.00 EUR`, or `29.90–35.00 EUR` for a range. */
function amountText(bill: BillSummary): string {
  const amount =
    bill.amount_min === bill.amount_max ? bill.amount_min : `${bill.amount_min}–${bill.amount_max}`;
  return `${amount} ${bill.currency}`;
}

/** Shows a notification for a `bill.due_soon` or `bill.overdue` notice. */
export async function alertBill(notice: EventNotice): Promise<void> {
  const title = TITLES[notice.type];
  if (!title || platform().kind !== "desktop") return;
  const bill: BillSummary = (
    await call(api.GET("/api/v1/bills/{id}", { params: { path: { id: notice.entity_id } } }))
  ).data;
  const body =
    notice.type === "bill.overdue"
      ? `${amountText(bill)} is overdue.`
      : `${amountText(bill)} due on ${bill.next_due_date ?? "its next due date"}.`;
  await platform().notify(`${title}: ${bill.name}`, body);
}
