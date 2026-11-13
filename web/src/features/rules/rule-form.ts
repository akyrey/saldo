// Rule editor state, the keyword catalogue, and conversions to API requests.
import type { components } from "@/api/schema";

export type RuleGroup = components["schemas"]["RuleGroupView"];
export type Rule = components["schemas"]["RuleView"];
type RuleRequest = components["schemas"]["RuleRequest"];

/** Which control a keyword's value needs. */
export type ValueKind =
  | "text"
  | "amount"
  | "currency"
  | "category"
  | "tag"
  | "account"
  | "kind"
  | "bill"
  | "budget"
  | "none";

interface Keyword {
  kind: string;
  label: string;
  value: ValueKind;
}

export const TRIGGER_KINDS: readonly Keyword[] = [
  { kind: "description_contains", label: "Description contains", value: "text" },
  { kind: "description_starts", label: "Description starts with", value: "text" },
  { kind: "description_ends", label: "Description ends with", value: "text" },
  { kind: "description_is", label: "Description is", value: "text" },
  { kind: "notes_contains", label: "Notes contain", value: "text" },
  { kind: "amount_less", label: "Amount less than", value: "amount" },
  { kind: "amount_more", label: "Amount more than", value: "amount" },
  { kind: "amount_exactly", label: "Amount is exactly", value: "amount" },
  { kind: "currency_is", label: "Currency is", value: "currency" },
  { kind: "category_is", label: "Category is", value: "category" },
  { kind: "has_no_category", label: "Has no category", value: "none" },
  { kind: "has_any_category", label: "Has a category", value: "none" },
  { kind: "tag_is", label: "Has tag", value: "tag" },
  { kind: "has_no_tag", label: "Has no tags", value: "none" },
  { kind: "has_any_tag", label: "Has a tag", value: "none" },
  { kind: "source_account_is", label: "Source account is", value: "account" },
  { kind: "destination_account_is", label: "Destination account is", value: "account" },
  { kind: "transaction_type", label: "Kind is", value: "kind" },
];

export const ACTION_KINDS: readonly Keyword[] = [
  { kind: "set_category", label: "Set category", value: "category" },
  { kind: "clear_category", label: "Clear category", value: "none" },
  { kind: "set_budget", label: "Set budget", value: "budget" },
  { kind: "clear_budget", label: "Clear budget", value: "none" },
  { kind: "add_tag", label: "Add tag", value: "tag" },
  { kind: "remove_tag", label: "Remove tag", value: "tag" },
  { kind: "remove_all_tags", label: "Remove all tags", value: "none" },
  { kind: "set_description", label: "Set description", value: "text" },
  { kind: "set_notes", label: "Set notes", value: "text" },
  { kind: "link_to_bill", label: "Link to bill", value: "bill" },
];

export const SPLIT_KINDS = [
  "withdrawal",
  "deposit",
  "transfer",
  "opening_balance",
  "reconciliation",
] as const;

/** The control for `kind`; unknown (imported) keywords get a text field. */
export function valueKind(kind: string): ValueKind {
  return [...TRIGGER_KINDS, ...ACTION_KINDS].find((k) => k.kind === kind)?.value ?? "text";
}

export interface TriggerRow {
  kind: string;
  value: string;
  negate: boolean;
}

export interface ActionRow {
  kind: string;
  value: string;
  stop_processing: boolean;
  active: boolean;
}

export interface RuleForm {
  rule_group_id: string;
  title: string;
  description: string;
  trigger_on: string;
  strict: boolean;
  stop_processing: boolean;
  active: boolean;
  sort_order: number;
  triggers: TriggerRow[];
  actions: ActionRow[];
}

export const emptyTrigger = (): TriggerRow => ({
  kind: "description_contains",
  value: "",
  negate: false,
});
export const emptyAction = (): ActionRow => ({
  kind: "set_category",
  value: "",
  stop_processing: false,
  active: true,
});

export function emptyRuleForm(groupId: string): RuleForm {
  return {
    rule_group_id: groupId,
    title: "",
    description: "",
    trigger_on: "create",
    strict: true,
    stop_processing: false,
    active: true,
    sort_order: 0,
    triggers: [emptyTrigger()],
    actions: [emptyAction()],
  };
}

export function fromRule(rule: Rule): RuleForm {
  return {
    rule_group_id: rule.rule_group_id,
    title: rule.title,
    description: rule.description ?? "",
    trigger_on: rule.trigger_on,
    strict: rule.strict,
    stop_processing: rule.stop_processing,
    active: rule.active,
    sort_order: rule.sort_order,
    triggers: rule.triggers.map((t) => ({
      kind: t.kind,
      value: t.value ?? "",
      negate: t.negate ?? false,
    })),
    actions: rule.actions.map((a) => ({
      kind: a.kind,
      value: a.value ?? "",
      stop_processing: a.stop_processing ?? false,
      active: a.active ?? true,
    })),
  };
}

/** Keywords without a value send `null`; others send the trimmed text. */
const valueOf = (kind: string, value: string) =>
  valueKind(kind) === "none" ? null : value.trim() || null;

export function toRequest(form: RuleForm): RuleRequest {
  return {
    rule_group_id: form.rule_group_id,
    title: form.title.trim(),
    description: form.description.trim() || null,
    trigger_on: form.trigger_on,
    strict: form.strict,
    stop_processing: form.stop_processing,
    active: form.active,
    sort_order: form.sort_order,
    triggers: form.triggers.map((t) => ({
      kind: t.kind,
      value: valueOf(t.kind, t.value),
      negate: t.negate,
    })),
    actions: form.actions.map((a) => ({
      kind: a.kind,
      value: valueOf(a.kind, a.value),
      stop_processing: a.stop_processing,
      active: a.active,
    })),
  };
}

/**
 * The list with `id` moved one place (`-1` up, `1` down) and every
 * `sort_order` renumbered 0, 1, 2…; unchanged when it cannot move.
 */
export function reorder<T extends { id: string; sort_order: number }>(
  items: readonly T[],
  id: string,
  step: -1 | 1,
): T[] {
  const from = items.findIndex((item) => item.id === id);
  const to = from + step;
  if (from < 0 || to < 0 || to >= items.length) return [...items];
  const moved = [...items];
  const [item] = moved.splice(from, 1);
  if (item) moved.splice(to, 0, item);
  return moved.map((entry, index) => ({ ...entry, sort_order: index }));
}
