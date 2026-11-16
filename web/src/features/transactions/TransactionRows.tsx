import { Link } from "@tanstack/react-router";
import { Pencil } from "lucide-react";
import type { ReactNode } from "react";
import { Amount } from "@/components/Amount";
import { TableCell, TableRow } from "@/components/ui/table";
import type { Account } from "@/features/accounts/queries";
import { CategoryDot } from "@/features/categories/CategoryDot";
import type { Category } from "@/features/categories/queries";
import { dayHeading } from "@/lib/dates";
import { cn } from "@/lib/utils";
import { AccountFlow } from "./AccountFlow";
import { groupTotals, signedAmount, splitFlow, type CurrencyTotal } from "./day-groups";
import { KindBadge } from "./KindBadge";
import type { Split, TransactionGroup } from "./queries";

export interface RowContext {
  accounts: ReadonlyMap<string, Account>;
  categories: ReadonlyMap<string, Category>;
  /** On an account's page, money leaving that account shows as negative. */
  perspectiveAccountId?: string;
  today: string;
}

/** Net totals per currency, one per line. */
export function Totals({ totals }: { totals: readonly CurrencyTotal[] }) {
  return (
    <span className="inline-flex flex-col items-end gap-0.5 text-sm font-medium">
      {totals.map((t) => (
        <Amount key={t.currency} value={t.amount} currency={t.currency} signed />
      ))}
    </span>
  );
}

/** Faded until hovered or focused, so upcoming transactions stay out of the way. */
export const FUTURE_ROW =
  "opacity-60 transition-opacity hover:opacity-100 focus-within:opacity-100";

function SplitCells({ split, ctx }: { split: Split; ctx: RowContext }) {
  const flow = splitFlow(split, ctx.perspectiveAccountId);
  const category = split.category_id ? ctx.categories.get(split.category_id) : undefined;
  return (
    <>
      <TableCell className="hidden sm:table-cell">
        <SplitAccounts split={split} ctx={ctx} />
      </TableCell>
      <TableCell className="hidden text-sm text-muted-foreground md:table-cell">
        {category ? (
          <span className="inline-flex items-center gap-1.5">
            <CategoryDot color={category.color} />
            {category.name}
          </span>
        ) : null}
      </TableCell>
      <TableCell className="hidden sm:table-cell">
        <KindBadge kind={split.kind} />
      </TableCell>
      <TableCell className="text-right align-top sm:align-middle">
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

function SplitAccounts({
  split,
  ctx,
  className,
}: {
  split: Split;
  ctx: RowContext;
  className?: string;
}) {
  return (
    <AccountFlow
      source={ctx.accounts.get(split.source_account_id)}
      destination={ctx.accounts.get(split.destination_account_id)}
      perspectiveAccountId={ctx.perspectiveAccountId}
      className={className}
    />
  );
}

/** The description cell; on phones the accounts follow below it (their column is hidden). */
function DescriptionCell(props: {
  split: Split;
  ctx: RowContext;
  className?: string;
  children: ReactNode;
}) {
  return (
    <TableCell className={cn("max-w-0 min-w-32 sm:max-w-none", props.className)}>
      <div className="break-words">{props.children}</div>
      <SplitAccounts split={props.split} ctx={props.ctx} className="mt-1 sm:hidden" />
    </TableCell>
  );
}

/** The whole description opens the editor on phones, where the edit column is hidden. */
function EditableText({ groupId, children }: { groupId: string; children: ReactNode }) {
  return (
    <Link
      to="/transactions/$groupId/edit"
      params={{ groupId }}
      className="rounded hover:underline focus-visible:underline"
    >
      {children}
    </Link>
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

function EditCell({ groupId }: { groupId?: string }) {
  return (
    <TableCell className="hidden sm:table-cell">
      {groupId ? <EditLink groupId={groupId} /> : null}
    </TableCell>
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
        <DescriptionCell split={first} ctx={ctx} className="pl-4">
          <EditableText groupId={group.id}>{first.description}</EditableText>
        </DescriptionCell>
        <SplitCells split={first} ctx={ctx} />
        <EditCell groupId={group.id} />
      </TableRow>
    );
  }
  return (
    <>
      <TableRow {...rowProps} className={cn(future && FUTURE_ROW)}>
        <TableCell className="pl-4 font-medium">
          <EditableText groupId={group.id}>{group.title ?? first.description}</EditableText>{" "}
          <span className="text-xs font-normal whitespace-nowrap text-muted-foreground">
            ({group.splits.length} splits)
          </span>
        </TableCell>
        <TableCell className="hidden sm:table-cell" />
        <TableCell className="hidden md:table-cell" />
        <TableCell className="hidden sm:table-cell" />
        <TableCell className="text-right" data-testid="group-total">
          <Totals totals={groupTotals(group, ctx.perspectiveAccountId)} />
        </TableCell>
        <EditCell groupId={group.id} />
      </TableRow>
      {group.splits.map((split) => (
        <TableRow
          key={split.id}
          className={cn("border-dashed text-muted-foreground", future && FUTURE_ROW)}
        >
          <DescriptionCell split={split} ctx={ctx} className="pl-8">
            {split.date === date ? null : (
              <span className="mr-2 text-xs whitespace-nowrap">
                {dayHeading(split.date, ctx.today)}
              </span>
            )}
            {split.description}
          </DescriptionCell>
          <SplitCells split={split} ctx={ctx} />
          <EditCell />
        </TableRow>
      ))}
    </>
  );
}
