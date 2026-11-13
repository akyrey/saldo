import { Amount } from "@/components/Amount";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

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

/** The exact numbers behind a chart, readable without the chart. */
export function ReportTable({ caption, firstTitle, labels, columns, currency }: ReportTableProps) {
  return (
    <div className="overflow-x-auto">
      <Table>
        <caption className="sr-only">{caption}</caption>
        <TableHeader>
          <TableRow>
            <TableHead>{firstTitle}</TableHead>
            {columns.map((c) => (
              <TableHead key={c.title} className="text-right">
                {c.title}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {labels.map((label, row) => (
            <TableRow key={`${label}-${row}`}>
              <TableCell>{label}</TableCell>
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
