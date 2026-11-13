import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type Me = components["schemas"]["MeView"];
export type Member = components["schemas"]["MemberView"];

export const meQuery = queryOptions({
  queryKey: ["me"],
  queryFn: async (): Promise<Me> => (await call(api.GET("/api/v1/auth/me"))).data,
  staleTime: 5 * 60_000,
});

export type AuthMethods = components["schemas"]["AuthMethods"];

/** Password login, an identity provider, or both. */
export const authMethodsQuery = queryOptions({
  queryKey: ["auth-methods"],
  queryFn: async (): Promise<AuthMethods> => (await call(api.GET("/api/v1/auth/methods"))).data,
  staleTime: 5 * 60_000,
});

export const setupStatusQuery = queryOptions({
  queryKey: ["setup-status"],
  queryFn: async (): Promise<boolean> => (await call(api.GET("/api/v1/setup"))).data.needed,
  staleTime: 0,
});

export const membersQuery = queryOptions({
  queryKey: ["members"],
  queryFn: async (): Promise<Member[]> => (await call(api.GET("/api/v1/household/members"))).data,
  staleTime: 5 * 60_000,
});
