import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";
import { useState } from "react";
import { Amount } from "@/components/Amount";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { nextSort, sortedRows, type TableSort } from "./table-sort";

export interface TableColumn {
  title: string;
  /** Decimal strings, one per row. */
  values: readonly string[];
}

interface ReportTableProps {
  caption: string;
  firstTitle: string;
  labels: readonly string[];
  columns: readonly TableColumn[];
  currency: string;
}

interface SortableHeadProps {
  title: string;
  column: number;
  sort: TableSort | null;
  onSort: (column: number) => void;
  alignRight?: boolean;
}

const ARIA_SORT = { asc: "ascending", desc: "descending" } as const;

/** A column header that sorts the table when clicked. */
function SortableHead({ title, column, sort, onSort, alignRight = false }: SortableHeadProps) {
  const direction = sort?.column === column ? sort.direction : null;
  const Icon = direction === "asc" ? ArrowUp : direction === "desc" ? ArrowDown : ArrowUpDown;
  return (
    <TableHead
      aria-sort={direction ? ARIA_SORT[direction] : "none"}
      className={alignRight ? "text-right" : undefined}
    >
      <button
        type="button"
        onClick={() => onSort(column)}
        className={cn(
          "inline-flex items-center gap-1 hover:text-foreground",
          direction && "text-foreground",
        )}
      >
        {title}
        <Icon aria-hidden className={cn("size-3.5", !direction && "opacity-40")} />
      </button>
    </TableHead>
  );
}

/** The exact numbers behind a chart, readable without the chart. Headers sort the rows. */
export function ReportTable({ caption, firstTitle, labels, columns, currency }: ReportTableProps) {
  const [sort, setSort] = useState<TableSort | null>(null);
  const onSort = (column: number) => setSort((current) => nextSort(current, column));
  const rows = sortedRows(labels, columns, sort);

  return (
    <div className="overflow-x-auto">
      <Table>
        <caption className="sr-only">{caption}</caption>
        <TableHeader>
          <TableRow>
            <SortableHead title={firstTitle} column={0} sort={sort} onSort={onSort} />
            {columns.map((c, index) => (
              <SortableHead
                key={c.title}
                title={c.title}
                column={index + 1}
                sort={sort}
                onSort={onSort}
                alignRight
              />
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row) => (
            <TableRow key={`${labels[row]}-${row}`}>
              <TableCell>{labels[row]}</TableCell>
              {columns.map((c) => (
                <TableCell key={c.title} className="text-right">
                  <Amount value={c.values[row] ?? "0"} currency={currency} />
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
