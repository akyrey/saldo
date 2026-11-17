import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { scopeParam, type Scope } from "@/features/shared/scope";

export type SubscriptionCandidate = components["schemas"]["SubscriptionCandidateView"];
export type AcceptCandidateRequest = components["schemas"]["AcceptCandidateRequest"];

export function candidatesQuery(scope?: Scope) {
  const value = scopeParam(scope);
  return queryOptions({
    queryKey: ["subscriptions", "candidates", value ?? "household"],
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/subscription-candidates", { params: { query: { scope: value } } }),
        )
      ).data,
  });
}
