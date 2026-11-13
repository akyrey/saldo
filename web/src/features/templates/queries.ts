import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";

export type Template = components["schemas"]["TransactionTemplateView"];
export type TemplateSplit = components["schemas"]["TransactionTemplateSplitView"];

/** Every template, by sort order then name (the server's order). */
export const templatesQuery = queryOptions({
  queryKey: ["templates", "list"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(
        api.GET("/api/v1/transaction-templates", {
          params: { query: { page, per_page: MAX_PER_PAGE } },
        }),
      ),
    ),
});

export function templateQuery(id: string) {
  return queryOptions({
    queryKey: ["templates", "one", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/transaction-templates/{id}", { params: { path: { id } } })))
        .data,
  });
}
