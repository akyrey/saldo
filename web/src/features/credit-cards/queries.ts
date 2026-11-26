import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import { ApiError } from "@/api/errors";
import type { components } from "@/api/schema";

export type CardPlan = components["schemas"]["CreditCardPlanView"];
export type CardCycle = components["schemas"]["CardCycleView"];
export type CardCycleDetail = components["schemas"]["CardCycleDetailView"];
export type CardSplit = components["schemas"]["CardSplitView"];
export type CardSummary = components["schemas"]["CardSummaryView"];

/** The card's plan, or `null` when it has none yet (a 404). */
export function cardPlanQuery(accountId: string) {
  return queryOptions({
    queryKey: ["credit-cards", "plan", accountId],
    queryFn: async (): Promise<CardPlan | null> => {
      try {
        const path = { id: accountId };
        return (await call(api.GET("/api/v1/accounts/{id}/credit-card", { params: { path } })))
          .data;
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return null;
        throw error;
      }
    },
  });
}

/** The card's months, newest first (the server's default range). */
export function cardCyclesQuery(accountId: string) {
  return queryOptions({
    queryKey: ["credit-cards", "cycles", accountId],
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/accounts/{id}/credit-card/cycles", {
            params: { path: { id: accountId } },
          }),
        )
      ).data,
  });
}

/** One month with the transactions that count toward it. */
export function cardCycleQuery(accountId: string, cycle: string) {
  return queryOptions({
    queryKey: ["credit-cards", "cycle", accountId, cycle],
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/accounts/{id}/credit-card/cycles/{cycle}", {
            params: { path: { id: accountId, cycle } },
          }),
        )
      ).data,
  });
}

/** The current month of every card with a plan, keyed by account ID. */
export const cardSummariesQuery = queryOptions({
  queryKey: ["credit-cards", "summaries"],
  queryFn: async (): Promise<ReadonlyMap<string, CardSummary>> => {
    const summaries = (await call(api.GET("/api/v1/credit-cards"))).data;
    return new Map(summaries.map((summary) => [summary.account_id, summary]));
  },
});
