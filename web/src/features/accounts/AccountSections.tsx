import { keepPreviousData, useInfiniteQuery } from "@tanstack/react-query";
import { useState } from "react";
import { QueryState } from "@/components/QueryState";
import { MAX_PER_PAGE } from "@/features/shared/pagination";
import type { Scope } from "@/features/shared/scope";
import { AccountGroupCard, AccountList, LoadMore, type Members, Totals } from "./AccountGroupCard";
import { flattenPages, groupAccounts } from "./grouping";
import { accountPagesQuery, type Currency } from "./queries";

/** Types shown open on arrival: few accounts, and the ones looked at daily. */
const OPEN_TYPES = ["asset", "liability"];
/** Expense and revenue accounts run into the hundreds; load them in pages. */
const FOLDED_PER_PAGE = 50;
const SEARCH_PER_PAGE = 50;

export interface SectionContext {
  scope?: Scope;
  /** `true` hides archived accounts. */
  active?: boolean;
  members: Members;
  currencies: readonly Currency[] | undefined;
}

/**
 * One card per account type. Assets and liabilities load at once, in full;
 * the other types load a page at a time once their card is opened.
 */
export function AccountSections({ context }: { context: SectionContext }) {
  return (
    <div className="grid items-start gap-4 lg:grid-cols-2">
      {["asset", "liability", "expense", "revenue", "equity"].map((type) => (
        <AccountSection key={type} type={type} context={context} />
      ))}
    </div>
  );
}

function AccountSection({ type, context }: { type: string; context: SectionContext }) {
  const startsOpen = OPEN_TYPES.includes(type);
  const [open, setOpen] = useState(startsOpen);
  const query = useInfiniteQuery({
    ...accountPagesQuery({
      scope: context.scope,
      active: context.active,
      type,
      perPage: startsOpen ? MAX_PER_PAGE : FOLDED_PER_PAGE,
    }),
    enabled: open,
  });
  const { accounts, balances, total } = flattenPages(query.data?.pages);
  // An empty liabilities card is noise; an empty asset card says so.
  if (startsOpen && query.data && total === 0 && type !== "asset") return null;

  const totals = groupAccounts(accounts, balances, context.currencies)[0]?.totals ?? [];
  let summary = null;
  if (query.data && query.hasNextPage) summary = `${accounts.length} of ${total}`;
  else if (query.data) summary = <Totals totals={totals} />;

  return (
    <AccountGroupCard
      type={type}
      summary={open ? summary : null}
      toggle={{ open, onToggle: () => setOpen((value) => !value) }}
    >
      <QueryState isPending={query.isPending} error={query.error} />
      {query.data && total === 0 ? (
        <p className="text-sm text-muted-foreground">No accounts yet.</p>
      ) : null}
      <AccountList accounts={accounts} balances={balances} members={context.members} />
      <LoadMore {...query} />
    </AccountGroupCard>
  );
}

/** Accounts of any type whose name contains `q`, grouped by type. */
export function AccountSearchResults({ q, context }: { q: string; context: SectionContext }) {
  const query = useInfiniteQuery({
    ...accountPagesQuery({
      scope: context.scope,
      active: context.active,
      q,
      perPage: SEARCH_PER_PAGE,
    }),
    // Keep the last results on screen while the next search loads.
    placeholderData: keepPreviousData,
  });
  const { accounts, balances, total } = flattenPages(query.data?.pages);
  const groups = groupAccounts(accounts, balances, context.currencies);

  return (
    <div className="grid gap-4">
      <QueryState isPending={query.isPending} error={query.error} />
      {query.data && !query.isPlaceholderData && total === 0 ? (
        <p className="text-sm text-muted-foreground">No accounts match “{q}”.</p>
      ) : null}
      <div className="grid items-start gap-4 lg:grid-cols-2">
        {groups.map((group) => (
          <AccountGroupCard key={group.type} type={group.type}>
            <AccountList accounts={group.accounts} balances={balances} members={context.members} />
          </AccountGroupCard>
        ))}
      </div>
      <LoadMore {...query} />
    </div>
  );
}
