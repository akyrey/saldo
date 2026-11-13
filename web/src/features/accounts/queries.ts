import { queryOptions } from "@tanstack/react-query";
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
