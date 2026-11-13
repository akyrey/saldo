import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";

export type Tag = components["schemas"]["TagView"];

export const tagsQuery = queryOptions({
  queryKey: ["tags"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(api.GET("/api/v1/tags", { params: { query: { page, per_page: MAX_PER_PAGE } } })),
    ),
});
