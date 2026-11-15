// Filling the transaction editor from something else: a template, or an
// existing transaction to duplicate. Pure functions, unit-tested.
import type { components } from "@/api/schema";
import { normalizeAmountInput } from "@/api/money";
import type { Template, TemplateSplit } from "@/features/templates/queries";
import {
  deriveKind,
  emptySplit,
  type AccountKindInfo,
  fromGroup,
  type EditorState,
  type SplitRow,
  type TemplateKind,
} from "./editor-model";
import type { TransactionGroup } from "./queries";

const TEMPLATE_KINDS: readonly TemplateKind[] = ["withdrawal", "deposit", "transfer"];

function templateKind(kind: string): TemplateKind | undefined {
  return TEMPLATE_KINDS.find((k) => k === kind);
}

function rowOf(split: TemplateSplit, today: string): SplitRow {
  return {
    ...emptySplit(today),
    description: split.description ?? "",
    source_id: split.source_id ?? "",
    destination_id: split.destination_id ?? "",
    amount: split.amount ?? "",
    foreign_amount: split.foreign_amount ?? "",
    foreign_currency: split.foreign_currency ?? "",
    category_id: split.category_id ?? "",
    budget_id: split.budget_id ?? "",
    bill_id: split.bill_id ?? "",
    tag_ids: [...(split.tag_ids ?? [])],
    notes: split.notes ?? "",
  };
}

/** A new transaction from a template, dated today. Missing parts stay empty. */
export function fromTemplate(template: Template, today: string): EditorState {
  const splits = template.splits.map((split) => rowOf(split, today));
  return {
    title: template.title ?? "",
    splits: splits.length > 0 ? splits : [emptySplit(today)],
    kind: templateKind(template.kind),
  };
}

/** A new, unsaved copy of a group, dated today (not reconciled, no attachments). */
export function duplicateOf(group: TransactionGroup, today: string): EditorState {
  const copy = fromGroup(group);
  // A copy continues no saved split (`id` dropped): it is a new transaction.
  return {
    ...copy,
    splits: copy.splits.map(({ id: _saved, ...row }) => ({ ...row, date: today })),
  };
}

/** Which account types each side of a kind may use. */
const SIDES: Record<TemplateKind, { source: readonly string[]; destination: readonly string[] }> = {
  withdrawal: { source: ["asset", "liability"], destination: ["expense"] },
  deposit: { source: ["revenue"], destination: ["asset", "liability"] },
  transfer: { source: ["asset", "liability"], destination: ["asset", "liability"] },
};

/** The account types to offer on each side, or `undefined` for all. */
export function accountTypesFor(
  kind: TemplateKind | undefined,
): { source: readonly string[]; destination: readonly string[] } | undefined {
  return kind ? SIDES[kind] : undefined;
}

export type TemplateRequest = components["schemas"]["CreateTransactionTemplateRequest"];

function orNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/**
 * The kind to save a template as: the editor's hint, else the first split
 * whose accounts form a template kind, else a withdrawal.
 */
export function templateKindOf(
  state: EditorState,
  accounts: readonly (AccountKindInfo & { id: string })[],
): TemplateKind {
  if (state.kind) return state.kind;
  for (const row of state.splits) {
    const find = (id: string) => accounts.find((a) => a.id === id);
    const kind = templateKind(deriveKind(find(row.source_id), find(row.destination_id)) ?? "");
    if (kind) return kind;
  }
  return "withdrawal";
}

/** The editor as a template: everything optional, dates left out. */
export function toTemplateRequest(
  state: EditorState,
  name: string,
  kind: TemplateKind,
): TemplateRequest {
  return {
    name: name.trim(),
    kind,
    title: orNull(state.title),
    splits: state.splits.map((row) => {
      const amount = orNull(row.amount);
      const foreign = orNull(row.foreign_amount);
      return {
        description: orNull(row.description),
        source_id: orNull(row.source_id),
        destination_id: orNull(row.destination_id),
        amount: amount === null ? null : normalizeAmountInput(amount),
        foreign_amount: foreign === null ? null : normalizeAmountInput(foreign),
        foreign_currency: orNull(row.foreign_currency),
        category_id: orNull(row.category_id),
        budget_id: orNull(row.budget_id),
        bill_id: orNull(row.bill_id),
        tag_ids: row.tag_ids,
        notes: orNull(row.notes),
      };
    }),
  };
}
