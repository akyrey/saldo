import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";
import { scopeParam, type Scope } from "@/features/shared/scope";

export type Account = components["schemas"]["AccountView"];
export type Balance = components["schemas"]["BalanceView"];
export type Currency = components["schemas"]["CurrencyView"];

/** Every account of the household (pickers and name lookups need them all). */
export function accountsQuery(scope?: Scope) {
  const scopeValue = scopeParam(scope);
  return queryOptions({
    queryKey: ["accounts", "all", scopeValue ?? "household"],
    queryFn: () =>
      fetchAllPages((page) =>
        call(
          api.GET("/api/v1/accounts", {
            params: { query: { page, per_page: MAX_PER_PAGE, scope: scopeValue } },
          }),
        ),
      ),
  });
}

/** Current balances of every account in the scope, keyed by account ID. */
export function balancesQuery(scope?: Scope) {
  const scopeValue = scopeParam(scope);
  return queryOptions({
    queryKey: ["balances", scopeValue ?? "household"],
    queryFn: async () => {
      const balances = await fetchAllPages((page) =>
        call(
          api.GET("/api/v1/accounts/balances", {
            params: { query: { page, per_page: MAX_PER_PAGE, scope: scopeValue } },
          }),
        ),
      );
      return new Map(balances.map((balance) => [balance.account_id, balance]));
    },
  });
}

/** What the accounts page asks for: one type, or a name search across types. */
export interface AccountListFilter {
  scope?: Scope;
  type?: string;
  /** `true` hides archived accounts; `undefined` shows them too. */
  active?: boolean;
  q?: string;
  perPage: number;
}

type ListMeta = components["schemas"]["ListMeta"];

/** One page of accounts with their balances, in the server's order. */
export interface AccountPage {
  accounts: Account[];
  balances: Balance[];
  meta: ListMeta;
}

/**
 * Accounts and their balances a page at a time, so the page never loads the
 * hundreds of expense and revenue accounts it is not showing. The key starts
 * with "balances" so that saving a transaction (which invalidates balances)
 * refreshes it; account changes invalidate both prefixes.
 */
export function accountPagesQuery(filter: AccountListFilter) {
  const { type, active, perPage } = filter;
  const scope = scopeParam(filter.scope);
  const q = filter.q?.trim() || undefined;
  return infiniteQueryOptions({
    queryKey: ["balances", "pages", { scope, type, active, q, perPage }],
    initialPageParam: 1,
    queryFn: async ({ pageParam }): Promise<AccountPage> => {
      const query = { page: pageParam, per_page: perPage, scope, type, active, q };
      const [accounts, balances] = await Promise.all([
        call(api.GET("/api/v1/accounts", { params: { query } })),
        call(api.GET("/api/v1/accounts/balances", { params: { query } })),
      ]);
      return { accounts: accounts.data, balances: balances.data, meta: accounts.meta };
    },
    getNextPageParam: (last, _pages, lastPage) =>
      lastPage * last.meta.per_page < last.meta.total ? lastPage + 1 : undefined,
  });
}

export function accountQuery(id: string) {
  return queryOptions({
    queryKey: ["accounts", "one", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/accounts/{id}", { params: { path: { id } } }))).data,
  });
}

export function accountBalanceQuery(id: string) {
  return queryOptions({
    queryKey: ["balances", "one", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/accounts/{id}/balance", { params: { path: { id } } }))).data,
  });
}

export const currenciesQuery = queryOptions({
  queryKey: ["currencies"],
  queryFn: async (): Promise<Currency[]> => (await call(api.GET("/api/v1/currencies"))).data,
  staleTime: Infinity,
});

/** Decimal places per currency code; 2 when unknown. */
export function placesOf(currencies: readonly Currency[] | undefined, code: string): number {
  return currencies?.find((currency) => currency.code === code)?.decimal_places ?? 2;
}
