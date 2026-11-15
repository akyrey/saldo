import { keepPreviousData, queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { scopeParam, type Scope } from "@/features/shared/scope";
import type { TransactionSearch } from "./search";

export type TransactionGroup = components["schemas"]["TransactionGroupView"];
export type Split = components["schemas"]["SplitView"];
export type TransactionRequest = components["schemas"]["TransactionRequest"];

export const PER_PAGE = 50;

export function transactionsQuery(filters: TransactionSearch, scope?: Scope) {
  const query = {
    ...filters,
    scope: scopeParam(scope),
    page: filters.page ?? 1,
    per_page: PER_PAGE,
  };
  return queryOptions({
    queryKey: ["transactions", "list", query],
    queryFn: () => call(api.GET("/api/v1/transactions", { params: { query } })),
    // Keep showing the old page while the next one loads.
    placeholderData: keepPreviousData,
  });
}

/** How many recent matches the description suggestions are picked from. */
const SUGGESTION_SOURCE_SIZE = 50;

/** Recent transactions whose description or title contains `q`, for suggestions. */
export function descriptionSearchQuery(q: string) {
  const query = { q, page: 1, per_page: SUGGESTION_SOURCE_SIZE };
  return queryOptions({
    // Not under "transactions": the editor awaits those refetches after a save.
    queryKey: ["description-suggestions", q],
    queryFn: () => call(api.GET("/api/v1/transactions", { params: { query } })),
    staleTime: 60_000,
    placeholderData: keepPreviousData,
  });
}

export function transactionQuery(id: string) {
  return queryOptions({
    queryKey: ["transactions", "one", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/transactions/{id}", { params: { path: { id } } }))).data,
  });
}
