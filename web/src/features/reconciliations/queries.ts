import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type Reconciliation = components["schemas"]["ReconciliationView"];
export type ReconciliationDraft = components["schemas"]["ReconciliationDraftView"];
export type DraftSplit = components["schemas"]["DraftSplitView"];

/** The account's reconciliations, latest first (one page is plenty to show). */
export function reconciliationsQuery(accountId: string) {
  return queryOptions({
    queryKey: ["reconciliations", accountId],
    queryFn: async () =>
      await call(
        api.GET("/api/v1/accounts/{id}/reconciliations", {
          params: { path: { id: accountId }, query: { per_page: 20 } },
        }),
      ),
  });
}

export function draftQuery(accountId: string, statementDate: string) {
  return queryOptions({
    queryKey: ["reconciliations", accountId, "draft", statementDate],
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/accounts/{id}/reconciliations/draft", {
            params: { path: { id: accountId }, query: { statement_date: statementDate } },
          }),
        )
      ).data,
  });
}
