import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { ReactNode } from "react";
import { Amount } from "@/components/Amount";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cardSummariesQuery, type CardSummary } from "@/features/credit-cards/queries";
import { ownerLabel, TYPE_LABELS } from "./grouping";
import type { Account, Balance } from "./queries";

export type Members = readonly { user_id: string; display_name: string }[] | undefined;

interface AccountGroupCardProps {
  type: string;
  /** Totals or a count, shown at the right of the title. */
  summary?: ReactNode;
  /** Set for a section that folds; `undefined` means always open. */
  toggle?: { open: boolean; onToggle: () => void };
  children?: ReactNode;
}

/** A card for one account type; its title folds the card when `toggle` is set. */
export function AccountGroupCard({ type, summary, toggle, children }: AccountGroupCardProps) {
  const label = TYPE_LABELS[type] ?? type;
  const open = toggle?.open ?? true;
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <Card data-testid={`group-${type}`}>
      <CardHeader className="flex-row items-baseline justify-between">
        <CardTitle>
          {toggle ? (
            <button
              type="button"
              aria-expanded={open}
              onClick={toggle.onToggle}
              className="flex items-center gap-1 hover:underline"
            >
              <Chevron className="size-4" aria-hidden />
              {label}
            </button>
          ) : (
            label
          )}
        </CardTitle>
        <div className="flex flex-col items-end text-sm">{summary}</div>
      </CardHeader>
      {open ? <CardContent>{children}</CardContent> : null}
    </Card>
  );
}

/** Totals per currency, one per line. */
export function Totals({ totals }: { totals: readonly { currency: string; amount: string }[] }) {
  return totals.map((total) => (
    <Amount key={total.currency} value={total.amount} currency={total.currency} />
  ));
}

interface AccountListProps {
  accounts: readonly Account[];
  balances: ReadonlyMap<string, Balance>;
  members: Members;
}

export function AccountList({ accounts, balances, members }: AccountListProps) {
  const hasCards = accounts.some((a) => a.asset_role === "credit_card");
  const cards = useQuery({ ...cardSummariesQuery, enabled: hasCards });
  return (
    <ul className="divide-y">
      {accounts.map((account) => (
        <AccountRow
          key={account.id}
          account={account}
          balance={balances.get(account.id)}
          card={cards.data?.get(account.id)}
          members={members}
        />
      ))}
    </ul>
  );
}

interface AccountRowProps {
  account: Account;
  balance?: Balance;
  /** The current month of a credit card with a plan. */
  card?: CardSummary;
  members: Members;
}

function AccountRow({ account, balance, card, members }: AccountRowProps) {
  return (
    <li className="flex items-center justify-between gap-2 py-2">
      <div className="min-w-0">
        <Link
          to="/accounts/$accountId"
          params={{ accountId: account.id }}
          className="font-medium hover:underline"
        >
          {account.name}
        </Link>
        <div className="flex gap-1 text-xs text-muted-foreground">
          {ownerLabel(account.owner_user_id, members)}
          {account.active ? null : <Badge>archived</Badge>}
        </div>
      </div>
      <div className="flex flex-col items-end">
        {balance ? (
          <Amount
            value={balance.balance}
            currency={balance.currency}
            className="text-sm"
            data-testid={`balance-${account.name}`}
          />
        ) : null}
        {card ? <CardLeft summary={card} name={account.name} /> : null}
      </div>
    </li>
  );
}

/** "€687.60 left of €1,500.00" under a card's balance. */
function CardLeft({ summary, name }: { summary: CardSummary; name: string }) {
  const { available, limit, currency } = summary.current;
  return (
    <span className="text-xs text-muted-foreground" data-testid={`card-left-${name}`}>
      <Amount value={available} currency={currency} /> left of{" "}
      <Amount value={limit} currency={currency} />
    </span>
  );
}

interface LoadMoreProps {
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => unknown;
}

/** "Load more" while the server has further pages. */
export function LoadMore({ hasNextPage, isFetchingNextPage, fetchNextPage }: LoadMoreProps) {
  if (!hasNextPage) return null;
  return (
    <Button
      variant="outline"
      size="sm"
      className="mt-2 w-full"
      disabled={isFetchingNextPage}
      onClick={() => void fetchNextPage()}
    >
      {isFetchingNextPage ? "Loading…" : "Load more"}
    </Button>
  );
}
