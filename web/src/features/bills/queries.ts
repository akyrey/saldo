import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";

export const billsQuery = queryOptions({
  queryKey: ["bills", "all"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(api.GET("/api/v1/bills", { params: { query: { page, per_page: MAX_PER_PAGE } } })),
    ),
});

export function billQuery(id: string) {
  return queryOptions({
    queryKey: ["bills", "one", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/bills/{id}", { params: { path: { id } } }))).data,
  });
}

export function periodsQuery(id: string) {
  return queryOptions({
    queryKey: ["bills", "periods", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/bills/{id}/periods", { params: { path: { id } } }))).data,
  });
}
