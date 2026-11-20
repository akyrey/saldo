import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type SavedView = components["schemas"]["SavedViewView"];
type Filters = Record<string, string | boolean>;

export const savedViewsQuery = queryOptions({
  queryKey: ["saved-views", "transactions"],
  queryFn: async () =>
    (await call(api.GET("/api/v1/saved-views", { params: { query: { page: "transactions" } } })))
      .data,
});

export async function createSavedView(name: string, filters: Filters): Promise<SavedView> {
  const body = { name, page: "transactions", filters };
  return (await call(api.POST("/api/v1/saved-views", { body }))).data;
}

export async function updateSavedView(
  id: string,
  patch: { name?: string; filters?: Filters },
): Promise<SavedView> {
  return (
    await call(api.PATCH("/api/v1/saved-views/{id}", { params: { path: { id } }, body: patch }))
  ).data;
}

export async function deleteSavedView(id: string): Promise<void> {
  await call(api.DELETE("/api/v1/saved-views/{id}", { params: { path: { id } } }));
}
