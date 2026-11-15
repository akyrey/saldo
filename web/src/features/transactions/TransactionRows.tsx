import { Link } from "@tanstack/react-router";
import { Pencil } from "lucide-react";
import { Amount } from "@/components/Amount";
import { TableCell, TableRow } from "@/components/ui/table";
import type { Account } from "@/features/accounts/queries";
import type { Category } from "@/features/categories/queries";
import { dayHeading } from "@/lib/dates";
import { cn } from "@/lib/utils";
import { signedAmount, splitFlow } from "./day-groups";
import { KindBadge } from "./KindBadge";
import type { Split, TransactionGroup } from "./queries";

export interface RowContext {
  accounts: ReadonlyMap<string, Account>;
  categories: ReadonlyMap<string, Category>;
  /** On an account's page, money leaving that account shows as negative. */
  perspectiveAccountId?: string;
  today: string;
}

/** Faded until hovered or focused, so upcoming transactions stay out of the way. */
export const FUTURE_ROW =
  "opacity-60 transition-opacity hover:opacity-100 focus-within:opacity-100";

function SplitCells({ split, ctx }: { split: Split; ctx: RowContext }) {
  const source = ctx.accounts.get(split.source_account_id);
  const destination = ctx.accounts.get(split.destination_account_id);
  const flow = splitFlow(split, ctx.perspectiveAccountId);
  const category = split.category_id ? ctx.categories.get(split.category_id) : undefined;
  return (
    <>
      <TableCell className="text-sm">
        {source?.name ?? "?"} → {destination?.name ?? "?"}
      </TableCell>
      <TableCell className="hidden text-sm text-muted-foreground md:table-cell">
        {category?.name}
      </TableCell>
      <TableCell className="hidden sm:table-cell">
        <KindBadge kind={split.kind} />
      </TableCell>
      <TableCell className="text-right">
        <span className="inline-flex items-center gap-1.5">
          <span className="sm:hidden">
            <KindBadge kind={split.kind} compact />
          </span>
          <Amount
            value={signedAmount(split.amount, flow)}
            currency={split.currency}
            signed={flow !== "neutral"}
          />
        </span>
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

interface GroupRowsProps {
  group: TransactionGroup;
  /** The day this group is listed under; splits on another date show theirs. */
  date: string;
  future: boolean;
  ctx: RowContext;
}

/** One transaction group: a single row, or a header row plus one row per split. */
export function GroupRows({ group, date, future, ctx }: GroupRowsProps) {
  const [first] = group.splits;
  if (!first) return null;
  const rowProps = {
    "data-testid": "transaction-row",
    "data-future": future || undefined,
  };
  if (group.splits.length === 1) {
    return (
      <TableRow {...rowProps} className={cn(future && FUTURE_ROW)}>
        <TableCell className="pl-4">{first.description}</TableCell>
        <SplitCells split={first} ctx={ctx} />
        <TableCell>
          <EditLink groupId={group.id} />
        </TableCell>
      </TableRow>
    );
  }
  return (
    <>
      <TableRow {...rowProps} className={cn(future && FUTURE_ROW)}>
        <TableCell colSpan={5} className="pl-4 font-medium">
          {group.title ?? first.description}{" "}
          <span className="text-xs text-muted-foreground">({group.splits.length} splits)</span>
        </TableCell>
        <TableCell>
          <EditLink groupId={group.id} />
        </TableCell>
      </TableRow>
      {group.splits.map((split) => (
        <TableRow key={split.id} className={cn("text-muted-foreground", future && FUTURE_ROW)}>
          <TableCell className="pl-8">
            {split.date === date ? null : (
              <span className="mr-2 text-xs whitespace-nowrap">
                {dayHeading(split.date, ctx.today)}
              </span>
            )}
            {split.description}
          </TableCell>
          <SplitCells split={split} ctx={ctx} />
          <TableCell />
        </TableRow>
      ))}
    </>
  );
}
