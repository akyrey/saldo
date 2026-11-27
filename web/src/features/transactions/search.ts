/** Transaction list filters, as they appear in the URL. */
export interface TransactionSearch {
  from?: string;
  to?: string;
  account_id?: string;
  /** A category ID, or `none` for withdrawals and deposits without one. */
  category_id?: string;
  tag_id?: string;
  /** A budget ID, or `none` for withdrawals without one. */
  budget_id?: string;
  bill_id?: string;
  kind?: SplitKind;
  reconciled?: boolean;
  /** Decimal strings with a dot, in the household's primary currency. */
  amount_min?: string;
  amount_max?: string;
  has_attachments?: boolean;
  q?: string;
  page?: number;
}

/** The value that asks for splits without a category or budget. */
export const NONE = "none";

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

const AMOUNT = /^\d+(\.\d+)?$/;

function pick(value: unknown, pattern: RegExp): string | undefined {
  return typeof value === "string" && pattern.test(value) ? value : undefined;
}

function idOrNone(value: unknown): string | undefined {
  return value === NONE ? NONE : pick(value, UUID);
}

/**
 * The router JSON-parses search params, so a hand-typed `?amount_min=100`
 * arrives as the number 100; turn finite numbers back into the text we store.
 */
function asText(value: unknown): unknown {
  return typeof value === "number" && Number.isFinite(value) ? String(value) : value;
}

/** A non-negative decimal; a comma is read as the decimal separator. */
function amount(value: unknown): string | undefined {
  const raw = asText(value);
  const text = typeof raw === "string" ? raw.trim().replace(",", ".") : raw;
  return pick(text, AMOUNT);
}

/** `true`/`false`, as a boolean or as the text a hand-edited URL gives. */
function flag(value: unknown): boolean | undefined {
  if (value === true || value === "true") return true;
  if (value === false || value === "false") return false;
  return undefined;
}

/**
 * Reads filters from the URL, dropping anything malformed so a hand-edited
 * link never sends a bad request.
 */
export function parseTransactionSearch(raw: Record<string, unknown>): TransactionSearch {
  const kind = SPLIT_KINDS.find((k) => k === raw.kind);
  const text = asText(raw.q);
  const q = typeof text === "string" && text.trim() !== "" ? text.trim() : undefined;
  const page = Number(raw.page);
  return {
    from: pick(raw.from, DATE),
    to: pick(raw.to, DATE),
    account_id: pick(raw.account_id, UUID),
    category_id: idOrNone(raw.category_id),
    tag_id: pick(raw.tag_id, UUID),
    budget_id: idOrNone(raw.budget_id),
    bill_id: pick(raw.bill_id, UUID),
    kind,
    reconciled: flag(raw.reconciled),
    amount_min: amount(raw.amount_min),
    amount_max: amount(raw.amount_max),
    has_attachments: flag(raw.has_attachments),
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

/** The filters folded behind the list's "Filters" button (search stays in view). */
const PANEL_FILTERS = [
  "from",
  "to",
  "account_id",
  "category_id",
  "tag_id",
  "budget_id",
  "bill_id",
  "kind",
  "reconciled",
  "amount_min",
  "amount_max",
  "has_attachments",
] as const;

/** How many of the folded filters are set, so the button can say so while they are hidden. */
export function activeFilterCount(value: TransactionSearch): number {
  return PANEL_FILTERS.filter((key) => value[key] !== undefined).length;
}

/** The filters a saved view stores: the set ones, without the page. */
export function viewFilters(value: TransactionSearch): Record<string, string | boolean> {
  const stored: Record<string, string | boolean> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (key === "page" || entry === undefined) continue;
    if (typeof entry === "string" || typeof entry === "boolean") stored[key] = entry;
  }
  return stored;
}

/** Whether two filter sets select the same transactions (the page aside). */
export function sameFilters(a: TransactionSearch, b: TransactionSearch): boolean {
  const left = viewFilters(a);
  const right = viewFilters(b);
  const keys = Object.keys(left);
  return keys.length === Object.keys(right).length && keys.every((k) => left[k] === right[k]);
}
