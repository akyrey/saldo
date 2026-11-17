// Edits to splits the bank already confirmed (reconciled), free of React.
import { normalizeAmountInput } from "@/api/money";
import type { EditorState, SplitRow } from "./editor-model";

export const RECONCILED_CONFIRM =
  "This changes a reconciled transaction's amount, date or accounts, or removes it. " +
  "It will no longer be reconciled. Save anyway?";

const confirmedFields = (row: SplitRow) => [
  row.date,
  normalizeAmountInput(row.amount),
  row.source_id,
  row.destination_id,
];

/** Whether saving `after` changes or removes a split in `reconciledIds`. */
export function touchesReconciled(
  before: EditorState,
  after: EditorState,
  reconciledIds: ReadonlySet<string>,
): boolean {
  return before.splits
    .filter((row) => row.id !== undefined && reconciledIds.has(row.id))
    .some((row) => {
      const now = after.splits.find((candidate) => candidate.id === row.id);
      if (!now) return true;
      const [was, is] = [confirmedFields(row), confirmedFields(now)];
      return was.some((value, index) => value !== is[index]);
    });
}
