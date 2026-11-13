import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";

export type Budget = components["schemas"]["BudgetView"];
export type BudgetLimit = components["schemas"]["BudgetLimitView"];
export type BudgetRequest = components["schemas"]["CreateBudgetRequest"];
export type BudgetPatch = components["schemas"]["BudgetPatch"];
export type LimitRequest = components["schemas"]["CreateLimitRequest"];

/** Every budget, active or not, in their display order. */
export const budgetsQuery = queryOptions({
  queryKey: ["budgets"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(api.GET("/api/v1/budgets", { params: { query: { page, per_page: MAX_PER_PAGE } } })),
    ),
});

export const budgetQuery = (id: string) =>
  queryOptions({
    queryKey: ["budgets", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/budgets/{id}", { params: { path: { id } } }))).data,
  });

/** The budget's limits overlapping `from..to` (both optional), newest first. */
export const limitsQuery = (budgetId: string, from?: string, to?: string) =>
  queryOptions({
    queryKey: ["budgets", budgetId, "limits", from ?? "", to ?? ""],
    queryFn: () =>
      fetchAllPages((page) =>
        call(
          api.GET("/api/v1/budgets/{id}/limits", {
            params: { path: { id: budgetId }, query: { from, to, page, per_page: MAX_PER_PAGE } },
          }),
        ),
      ),
  });
