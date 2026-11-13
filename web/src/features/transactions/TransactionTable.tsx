import { Link } from "@tanstack/react-router";
import { Pencil } from "lucide-react";
import { Amount } from "@/components/Amount";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { Account } from "@/features/accounts/queries";
import type { Category } from "@/features/categories/queries";
import { KIND_LABELS } from "./editor-model";
import type { Split, TransactionGroup } from "./queries";

interface TransactionTableProps {
  groups: readonly TransactionGroup[];
  accounts: ReadonlyMap<string, Account>;
  categories: ReadonlyMap<string, Category>;
  /** On an account's page, money leaving that account shows as negative. */
  perspectiveAccountId?: string;
}

function signed(split: Split, perspective: string | undefined): string {
  return perspective && split.source_account_id === perspective ? `-${split.amount}` : split.amount;
}

function SplitCells({ split, props }: { split: Split; props: TransactionTableProps }) {
  const source = props.accounts.get(split.source_account_id);
  const destination = props.accounts.get(split.destination_account_id);
  const kind = KIND_LABELS[split.kind as keyof typeof KIND_LABELS] ?? split.kind;
  return (
    <>
      <TableCell className="text-sm">
        {source?.name ?? "?"} → {destination?.name ?? "?"}
      </TableCell>
      <TableCell className="hidden text-sm text-muted-foreground md:table-cell">
        {split.category_id ? props.categories.get(split.category_id)?.name : null}
      </TableCell>
      <TableCell className="hidden sm:table-cell">
        <Badge>{kind}</Badge>
      </TableCell>
      <TableCell className="text-right">
        <Amount value={signed(split, props.perspectiveAccountId)} currency={split.currency} />
      </TableCell>
    </>
  );
}

function EditLink({ groupId }: { groupId: string }) {
  return (
    <Link
      to="/transactions/$groupId/edit"
      params={{ groupId }}
      aria-label="Edit transaction"
      className="inline-flex rounded p-1 text-muted-foreground hover:bg-accent"
    >
      <Pencil className="size-4" />
    </Link>
  );
}

/** Transaction groups; a group with several splits shows a header and every split. */
export function TransactionTable(props: TransactionTableProps) {
  if (props.groups.length === 0) {
    return <p className="py-6 text-sm text-muted-foreground">No transactions match.</p>;
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Date</TableHead>
          <TableHead>Description</TableHead>
          <TableHead>From → To</TableHead>
          <TableHead className="hidden md:table-cell">Category</TableHead>
          <TableHead className="hidden sm:table-cell">Kind</TableHead>
          <TableHead className="text-right">Amount</TableHead>
          <TableHead>
            <span className="sr-only">Edit</span>
          </TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {props.groups.flatMap((group) => {
          const [first] = group.splits;
          if (!first) return [];
          if (group.splits.length === 1) {
            return [
              <TableRow key={group.id} data-testid="transaction-row">
                <TableCell className="whitespace-nowrap">{first.date}</TableCell>
                <TableCell>{first.description}</TableCell>
                <SplitCells split={first} props={props} />
                <TableCell>
                  <EditLink groupId={group.id} />
                </TableCell>
              </TableRow>,
            ];
          }
          return [
            <TableRow key={group.id} className="bg-muted/40" data-testid="transaction-row">
              <TableCell className="whitespace-nowrap">{first.date}</TableCell>
              <TableCell colSpan={5} className="font-medium">
                {group.title ?? first.description}{" "}
                <span className="text-xs text-muted-foreground">
                  ({group.splits.length} splits)
                </span>
              </TableCell>
              <TableCell>
                <EditLink groupId={group.id} />
              </TableCell>
            </TableRow>,
            ...group.splits.map((split) => (
              <TableRow key={split.id}>
                <TableCell className="whitespace-nowrap pl-6 text-muted-foreground">
                  {split.date === first.date ? "" : split.date}
                </TableCell>
                <TableCell className="pl-6">{split.description}</TableCell>
                <SplitCells split={split} props={props} />
                <TableCell />
              </TableRow>
            )),
          ];
        })}
      </TableBody>
    </Table>
  );
}
