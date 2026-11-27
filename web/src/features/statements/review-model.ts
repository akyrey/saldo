// Plain-language labels and small calculations for the statement review.

const STATUS_LABELS: Record<string, string> = {
  new: "New",
  duplicate: "Already imported",
  possible_match: "Matches a transaction",
  skipped_before: "Skipped before",
  not_matched_reconciled: "Missing in a reconciled period",
};

const FINDING_LABELS: Record<string, string> = {
  duplicate_of: "Probably a duplicate",
  identical: "Identical transactions",
  not_on_statement: "Not on the statement",
};

/** What a line's status means, for the review table. */
export function statusLabel(status: string): string {
  return STATUS_LABELS[status] ?? status;
}

/** What a finding about an existing transaction means. */
export function findingLabel(kind: string): string {
  return FINDING_LABELS[kind] ?? kind;
}

export interface ActionCounts {
  create: number;
  link: number;
  skip: number;
}

/** How many lines a commit will create, link and skip. */
export function actionCounts(lines: readonly { action: string }[]): ActionCounts {
  return lines.reduce<ActionCounts>(
    (counts, line) =>
      line.action === "create" || line.action === "link" || line.action === "skip"
        ? { ...counts, [line.action]: counts[line.action] + 1 }
        : counts,
    { create: 0, link: 0, skip: 0 },
  );
}

/** The reconcile page's search for a committed import, when the file had a closing balance. */
export function reconcileSearch(view: {
  closing_date?: string | null;
  closing_balance?: string | null;
}): { date: string; balance: string } | null {
  if (!view.closing_date || !view.closing_balance) return null;
  return { date: view.closing_date, balance: view.closing_balance };
}

/** The first name of each part column (Satispay's vouchers) a mapping names. */
export function partColumns(mapping: unknown): string[] {
  if (typeof mapping !== "object" || mapping === null) return [];
  const columns = (mapping as { part_columns?: unknown }).part_columns;
  if (!Array.isArray(columns)) return [];
  return columns
    .map((names: unknown) => (Array.isArray(names) ? names[0] : undefined))
    .filter((name): name is string => typeof name === "string");
}

/** The name an account created for this line gets: its counterparty, or its text. */
export function newCounterpartyName(line: {
  counterparty_name?: string | null;
  description: string;
}): string {
  const name = line.counterparty_name?.trim();
  return name ? name : line.description;
}

export interface Finding {
  kind: string;
  date: string;
  amount: string;
  description: string;
}

export interface UnsupportedRow {
  row: number;
  reason: string;
}

const isString = (value: unknown): value is string => typeof value === "string";

function isFinding(value: unknown): value is Finding {
  const f = value as Partial<Record<keyof Finding, unknown>> | null;
  return (
    typeof f === "object" &&
    f !== null &&
    isString(f.kind) &&
    isString(f.date) &&
    isString(f.amount) &&
    isString(f.description)
  );
}

function isUnsupported(value: unknown): value is UnsupportedRow {
  const u = value as Partial<Record<keyof UnsupportedRow, unknown>> | null;
  return typeof u === "object" && u !== null && typeof u.row === "number" && isString(u.reason);
}

/** The findings and skipped rows of an import's summary, checked one by one. */
export function summaryNotes(summary: unknown): {
  findings: Finding[];
  unsupported: UnsupportedRow[];
  more: number;
} {
  const s = (typeof summary === "object" && summary !== null ? summary : {}) as Record<
    string,
    unknown
  >;
  const findings = Array.isArray(s.findings) ? s.findings.filter(isFinding) : [];
  const unsupported = Array.isArray(s.unsupported) ? s.unsupported.filter(isUnsupported) : [];
  const total = typeof s.unsupported_total === "number" ? s.unsupported_total : 0;
  return { findings, unsupported, more: Math.max(0, total - unsupported.length) };
}

export interface Proposal {
  source: string;
  description: string;
  counterparty: string;
  notes: string[];
  items: { product: string; amount_minor: number }[];
}

/** A line's proposal from the server, checked; `null` when there is none. */
export function proposalOf(value: unknown): Proposal | null {
  const p = value as Partial<Record<keyof Proposal, unknown>> | null;
  if (typeof p !== "object" || p === null) return null;
  if (!isString(p.source) || !isString(p.description) || !isString(p.counterparty)) return null;
  const notes = Array.isArray(p.notes) ? p.notes.filter(isString) : [];
  const items = Array.isArray(p.items)
    ? p.items.filter(
        (i: unknown): i is { product: string; amount_minor: number } =>
          typeof i === "object" &&
          i !== null &&
          isString((i as { product?: unknown }).product) &&
          typeof (i as { amount_minor?: unknown }).amount_minor === "number",
      )
    : [];
  return {
    source: p.source,
    description: p.description,
    counterparty: p.counterparty,
    notes,
    items,
  };
}
