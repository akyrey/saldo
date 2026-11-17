// Subscription suggestions for the screens, free of React.
import { normalizeAmountInput } from "@/api/money";
import type { AcceptCandidateRequest, SubscriptionCandidate } from "./queries";

const FREQUENCY_LABELS: Record<string, string> = {
  weekly: "Weekly",
  monthly: "Monthly",
  quarterly: "Every three months",
  half_year: "Every six months",
  yearly: "Yearly",
};

export function frequencyLabel(frequency: string): string {
  return FREQUENCY_LABELS[frequency] ?? frequency;
}

/** What the "Add as bill" form holds: every input is a string. */
export interface SuggestionForm {
  name: string;
  amountMin: string;
  amountMax: string;
}

export function suggestionForm(candidate: SubscriptionCandidate): SuggestionForm {
  return {
    name: candidate.account_name,
    amountMin: candidate.amount_min,
    amountMax: candidate.amount_max,
  };
}

export function acceptRequest(
  candidate: SubscriptionCandidate,
  form: SuggestionForm,
): AcceptCandidateRequest {
  return {
    account_id: candidate.account_id,
    currency: candidate.currency,
    name: form.name.trim(),
    frequency: candidate.frequency,
    amount_min: normalizeAmountInput(form.amountMin),
    amount_max: normalizeAmountInput(form.amountMax),
    split_ids: candidate.split_ids,
  };
}
