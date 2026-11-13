import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";

export const ruleGroupsQuery = queryOptions({
  queryKey: ["rules", "groups"],
  queryFn: async () => (await call(api.GET("/api/v1/rule-groups"))).data,
});

export const rulesQuery = queryOptions({
  queryKey: ["rules", "all"],
  queryFn: async () => (await call(api.GET("/api/v1/rules"))).data,
});

export function ruleQuery(id: string) {
  return queryOptions({
    queryKey: ["rules", "one", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/rules/{id}", { params: { path: { id } } }))).data,
  });
}
