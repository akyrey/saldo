import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";
import type { TagMerge } from "./duplicates";

export type Tag = components["schemas"]["TagView"];

export const tagsQuery = queryOptions({
  queryKey: ["tags"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(api.GET("/api/v1/tags", { params: { query: { page, per_page: MAX_PER_PAGE } } })),
    ),
});

/** Under `["tags"]`, so every tag change refreshes it too. */
export const duplicatesQuery = queryOptions({
  queryKey: ["tags", "duplicates"],
  queryFn: async () => (await call(api.GET("/api/v1/tags/duplicates"))).data,
});

/** Runs the merges in one transaction; returns the tags that were kept. */
export async function mergeTags(merges: TagMerge[]): Promise<Tag[]> {
  return (await call(api.POST("/api/v1/tags/merge", { body: { merges } }))).data;
}
