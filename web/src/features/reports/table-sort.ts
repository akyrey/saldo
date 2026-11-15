// Sorting the rows of a report table, free of React. Column 0 is the label
// column; columns 1.. are the amount columns, in the order they are shown.
import { toMinor } from "@/api/money";

export type SortDirection = "asc" | "desc";

export interface TableSort {
  column: number;
  direction: SortDirection;
}

/** The sort after a click on `column`: ascending, then descending, then none. */
export function nextSort(current: TableSort | null, column: number): TableSort | null {
  if (current?.column !== column) return { column, direction: "asc" };
  return current.direction === "asc" ? { column, direction: "desc" } : null;
}

const FRACTION = /\.(\d+)$/;

/**
 * Decimal strings as `bigint` minor units, all at the same scale so they
 * compare exactly. Invalid or missing values count as zero.
 */
function minorValues(values: readonly string[], rows: number): bigint[] {
  const places = Math.max(0, ...values.map((v) => FRACTION.exec(v)?.[1]?.length ?? 0));
  return Array.from({ length: rows }, (_, row) => toMinor(values[row] ?? "0", places) ?? 0n);
}

function compareBigints(a: bigint, b: bigint): number {
  if (a === b) return 0;
  return a < b ? -1 : 1;
}

/**
 * The row indices in display order. Unsorted keeps the server's order; ties
 * keep it too, because `toSorted` is stable.
 */
export function sortedRows(
  labels: readonly string[],
  columns: readonly { values: readonly string[] }[],
  sort: TableSort | null,
): number[] {
  const rows = labels.map((_, row) => row);
  if (sort === null) return rows;

  const sign = sort.direction === "asc" ? 1 : -1;
  if (sort.column === 0) {
    const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
    return rows.toSorted((a, b) => sign * collator.compare(labels[a] ?? "", labels[b] ?? ""));
  }

  const amounts = minorValues(columns[sort.column - 1]?.values ?? [], labels.length);
  return rows.toSorted((a, b) => sign * compareBigints(amounts[a] ?? 0n, amounts[b] ?? 0n));
}
