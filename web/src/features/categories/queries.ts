import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";

export type Category = components["schemas"]["CategoryView"];

export const categoriesQuery = queryOptions({
  queryKey: ["categories"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(api.GET("/api/v1/categories", { params: { query: { page, per_page: MAX_PER_PAGE } } })),
    ),
});
