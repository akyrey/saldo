/** Transaction list filters, as they appear in the URL. */
export interface TransactionSearch {
  from?: string;
  to?: string;
  account_id?: string;
  category_id?: string;
  tag_id?: string;
  kind?: SplitKind;
  q?: string;
  page?: number;
}

export const SPLIT_KINDS = [
  "withdrawal",
  "deposit",
  "transfer",
  "opening_balance",
  "reconciliation",
] as const;

export type SplitKind = (typeof SPLIT_KINDS)[number];

const DATE = /^\d{4}-\d{2}-\d{2}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function pick(value: unknown, pattern: RegExp): string | undefined {
  return typeof value === "string" && pattern.test(value) ? value : undefined;
}

/**
 * Reads filters from the URL, dropping anything malformed so a hand-edited
 * link never sends a bad request.
 */
export function parseTransactionSearch(raw: Record<string, unknown>): TransactionSearch {
  const kind = SPLIT_KINDS.find((k) => k === raw.kind);
  const q = typeof raw.q === "string" && raw.q.trim() !== "" ? raw.q.trim() : undefined;
  const page = Number(raw.page);
  return {
    from: pick(raw.from, DATE),
    to: pick(raw.to, DATE),
    account_id: pick(raw.account_id, UUID),
    category_id: pick(raw.category_id, UUID),
    tag_id: pick(raw.tag_id, UUID),
    kind,
    q,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
  };
}

/** What a new transaction starts from: a template or a group to duplicate. */
export interface NewTransactionSearch {
  template?: string;
  duplicate?: string;
}

export function parseNewTransactionSearch(search: Record<string, unknown>): NewTransactionSearch {
  return { template: pick(search.template, UUID), duplicate: pick(search.duplicate, UUID) };
}
