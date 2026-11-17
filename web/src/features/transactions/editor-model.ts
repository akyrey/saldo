// The transaction editor's state and rules, kept free of React so they can
// be unit-tested. Every function returns a new state; nothing is mutated.
import type { ErrorDetail } from "@/api/errors";
import { normalizeAmountInput } from "@/api/money";
import type { TransactionGroup, TransactionRequest } from "./queries";
import type { SplitKind } from "./search";

/** One split row as the form holds it: every input is a string. */
export interface SplitRow {
  /** Stable React key; not sent to the server. */
  key: string;
  /**
   * The saved split this row continues (loaded rows only). Sent on save, so
   * the split keeps its ID and attachments; a row without one is new.
   */
  id?: string;
  date: string;
  description: string;
  source_id: string;
  destination_id: string;
  amount: string;
  foreign_amount: string;
  foreign_currency: string;
  category_id: string;
  /** Only withdrawals may have a budget. */
  budget_id: string;
  /** The bill this split pays; it marks the bill's matching period paid. */
  bill_id: string;
  tag_ids: string[];
  notes: string;
}

/** The kinds a transaction template can have. */
export type TemplateKind = "withdrawal" | "deposit" | "transfer";

export interface EditorState {
  title: string;
  splits: SplitRow[];
  /** From a template: limits the account pickers to this kind's accounts. */
  kind?: TemplateKind;
}

/** What kind derivation needs to know about an account. */
export interface AccountKindInfo {
  type: string;
  equity_role?: string | null;
}

let nextKey = 0;
export function newKey(): string {
  nextKey += 1;
  return `split-${nextKey}`;
}

export function emptySplit(date: string): SplitRow {
  return {
    key: newKey(),
    date,
    description: "",
    source_id: "",
    destination_id: "",
    amount: "",
    foreign_amount: "",
    foreign_currency: "",
    category_id: "",
    budget_id: "",
    bill_id: "",
    tag_ids: [],
    notes: "",
  };
}

/** Adds a row after the last one, reusing its date (splits are usually same-day). */
export function addSplit(state: EditorState, today: string): EditorState {
  const last = state.splits.at(-1);
  return { ...state, splits: [...state.splits, emptySplit(last?.date ?? today)] };
}

export function updateSplit(
  state: EditorState,
  index: number,
  changes: Partial<Omit<SplitRow, "key">>,
): EditorState {
  return {
    ...state,
    splits: state.splits.map((row, i) => (i === index ? { ...row, ...changes } : row)),
  };
}

/** Removes a row; a group keeps at least one. */
export function removeSplit(state: EditorState, index: number): EditorState {
  if (state.splits.length <= 1) return state;
  return { ...state, splits: state.splits.filter((_, i) => i !== index) };
}

export function fromGroup(group: TransactionGroup): EditorState {
  return {
    title: group.title ?? "",
    splits: group.splits.map((split) => ({
      key: newKey(),
      id: split.id,
      date: split.date,
      description: split.description,
      source_id: split.source_account_id,
      destination_id: split.destination_account_id,
      amount: split.amount,
      foreign_amount: split.foreign_amount ?? "",
      foreign_currency: split.foreign_currency ?? "",
      category_id: split.category_id ?? "",
      budget_id: split.budget_id ?? "",
      bill_id: split.bill_id ?? "",
      tag_ids: [...split.tag_ids],
      notes: split.notes ?? "",
    })),
  };
}

function orNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** The request body. Amounts stay strings; the server validates them. */
export function toRequest(state: EditorState): TransactionRequest {
  return {
    title: orNull(state.title),
    splits: state.splits.map((row) => {
      const foreignAmount = orNull(row.foreign_amount);
      return {
        ...(row.id === undefined ? {} : { id: row.id }),
        date: row.date,
        description: row.description.trim(),
        source_id: row.source_id,
        destination_id: row.destination_id,
        amount: normalizeAmountInput(row.amount),
        foreign_amount: foreignAmount === null ? null : normalizeAmountInput(foreignAmount),
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

function isAssetOrLiability(account: AccountKindInfo): boolean {
  return account.type === "asset" || account.type === "liability";
}

/**
 * The split kind for a pair of accounts, mirroring `SplitKind::derive` in
 * saldo-core (ARCHITECTURE.md 4.1). `undefined` until both are chosen;
 * `"invalid"` for a pair the server will reject.
 */
export function deriveKind(
  source: AccountKindInfo | undefined,
  destination: AccountKindInfo | undefined,
): SplitKind | "invalid" | undefined {
  if (!source || !destination) return undefined;
  if (isAssetOrLiability(source) && destination.type === "expense") return "withdrawal";
  if (source.type === "revenue" && isAssetOrLiability(destination)) return "deposit";
  if (isAssetOrLiability(source) && isAssetOrLiability(destination)) return "transfer";
  const [equity, other] = source.type === "equity" ? [source, destination] : [destination, source];
  if (equity.type === "equity" && isAssetOrLiability(other)) {
    if (equity.equity_role === "opening_balance") return "opening_balance";
    if (equity.equity_role === "reconciliation") return "reconciliation";
  }
  return "invalid";
}

/** Field errors per row (`row` holds errors about the whole split) plus the rest. */
export interface EditorErrors {
  rows: Record<number, Record<string, string>>;
  form: string[];
}

const SPLIT_FIELD = /^splits\[(\d+)\](?:\.(\w+))?$/;

/** Sorts the server's `details[].field` paths (e.g. `splits[1].amount`) onto rows. */
export function mapServerErrors(details: readonly ErrorDetail[]): EditorErrors {
  const errors: EditorErrors = { rows: {}, form: [] };
  for (const detail of details) {
    const match = detail.field ? SPLIT_FIELD.exec(detail.field) : null;
    if (!match) {
      errors.form.push(detail.field ? `${detail.field}: ${detail.message}` : detail.message);
      continue;
    }
    const index = Number(match[1]);
    const field = match[2] ?? "row";
    const row = errors.rows[index] ?? {};
    if (!(field in row)) row[field] = detail.message;
    errors.rows[index] = row;
  }
  return errors;
}

export const KIND_LABELS: Record<SplitKind | "invalid", string> = {
  withdrawal: "Withdrawal",
  deposit: "Deposit",
  transfer: "Transfer",
  opening_balance: "Opening balance",
  reconciliation: "Reconciliation",
  invalid: "Not allowed",
};
