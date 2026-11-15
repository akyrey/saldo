import { ChevronDown, ChevronRight } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Amount } from "@/components/Amount";
import { DateTile } from "@/components/DateTile";
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
import { dayName, todayIso } from "@/lib/dates";
import { cn } from "@/lib/utils";
import { groupByDay, type CurrencyTotal, type DayGroup } from "./day-groups";
import type { TransactionGroup } from "./queries";
import { FUTURE_ROW, GroupRows, type RowContext } from "./TransactionRows";

interface TransactionTableProps {
  groups: readonly TransactionGroup[];
  accounts: ReadonlyMap<string, Account>;
  categories: ReadonlyMap<string, Category>;
  /** On an account's page, money leaving that account shows as negative. */
  perspectiveAccountId?: string;
  /** Today as `YYYY-MM-DD`; later days go in the "Upcoming" section. Defaults to the clock. */
  today?: string;
}

function Totals({ totals }: { totals: readonly CurrencyTotal[] }) {
  return (
    <span className="inline-flex flex-col items-end gap-0.5 text-sm font-medium">
      {totals.map((t) => (
        <Amount key={t.currency} value={t.amount} currency={t.currency} signed />
      ))}
    </span>
  );
}

/**
 * A row whose label spans the description and accounts columns, with totals in the
 * amount column. The hidden cells keep the columns aligned on narrow screens.
 */
function SummaryRow(props: {
  label: ReactNode;
  totals: readonly CurrencyTotal[];
  className?: string;
  testId?: string;
}) {
  return (
    <TableRow data-testid={props.testId} className={cn("hover:bg-transparent", props.className)}>
      <TableCell colSpan={2} className="pt-4 pb-1">
        {props.label}
      </TableCell>
      <TableCell className="hidden md:table-cell" />
      <TableCell className="hidden sm:table-cell" />
      <TableCell className="pt-4 pb-1 text-right align-bottom">
        <Totals totals={props.totals} />
      </TableCell>
      <TableCell />
    </TableRow>
  );
}

function DaySection({ day, future, ctx }: { day: DayGroup; future: boolean; ctx: RowContext }) {
  const label = (
    <span className="flex items-center gap-3">
      <DateTile date={day.date} today={ctx.today} />
      <span className="font-medium">{dayName(day.date, ctx.today)}</span>
    </span>
  );
  return (
    <>
      <SummaryRow
        label={label}
        totals={day.totals}
        testId="day-heading"
        className={cn(future && FUTURE_ROW)}
      />
      {day.groups.map((group) => (
        <GroupRows key={group.id} group={group} date={day.date} future={future} ctx={ctx} />
      ))}
    </>
  );
}

function UpcomingToggle(props: {
  open: boolean;
  count: number;
  totals: readonly CurrencyTotal[];
  onToggle: () => void;
}) {
  const Chevron = props.open ? ChevronDown : ChevronRight;
  const label = (
    <button
      type="button"
      aria-expanded={props.open}
      onClick={props.onToggle}
      className="inline-flex items-center gap-2 rounded text-sm font-medium text-muted-foreground hover:text-foreground"
    >
      <Chevron className="size-4" aria-hidden />
      Upcoming
      <span className="rounded-full bg-muted px-2 py-0.5 text-xs">{props.count}</span>
    </button>
  );
  return (
    <SummaryRow
      label={label}
      totals={props.totals}
      className="border-dashed text-muted-foreground"
    />
  );
}

/** Transactions by day, newest first; days after today fold into an "Upcoming" section. */
export function TransactionTable(props: TransactionTableProps) {
  // null until the person toggles it: closed, unless the page has nothing else to show.
  const [upcomingToggled, setUpcomingToggled] = useState<boolean | null>(null);
  if (props.groups.length === 0) {
    return <p className="py-6 text-sm text-muted-foreground">No transactions match.</p>;
  }
  const ctx: RowContext = { ...props, today: props.today ?? todayIso() };
  const days = groupByDay(props.groups, ctx.today, props.perspectiveAccountId);
  const upcomingCount = days.upcoming.reduce((n, day) => n + day.groups.length, 0);
  const upcomingOpen = upcomingToggled ?? days.past.length === 0;
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="pl-4">Description</TableHead>
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
        {upcomingCount > 0 ? (
          <UpcomingToggle
            open={upcomingOpen}
            count={upcomingCount}
            totals={days.upcomingTotals}
            onToggle={() => setUpcomingToggled(!upcomingOpen)}
          />
        ) : null}
        {upcomingOpen
          ? days.upcoming.map((day) => <DaySection key={day.date} day={day} future ctx={ctx} />)
          : null}
        {days.past.map((day) => (
          <DaySection key={day.date} day={day} future={false} ctx={ctx} />
        ))}
      </TableBody>
    </Table>
  );
}
