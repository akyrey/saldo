import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type Channel = components["schemas"]["ChannelView"];
export type CreateChannel = components["schemas"]["CreateChannelRequest"];

export const channelsQuery = queryOptions({
  queryKey: ["notification-channels"],
  queryFn: async () => (await call(api.GET("/api/v1/notification-channels"))).data,
});

/** `null` when the server has no VAPID key (Web Push is off). */
export const vapidKeyQuery = queryOptions({
  queryKey: ["vapid-key"],
  queryFn: async () => (await call(api.GET("/api/v1/push/vapid-key"))).data.public_key ?? null,
  staleTime: Infinity,
});

export type NotificationType = components["schemas"]["NotificationType"];

/** The event types a channel can receive, in display order. */
export const notificationTypesQuery = queryOptions({
  queryKey: ["notification-types"],
  queryFn: async () => (await call(api.GET("/api/v1/notification-types"))).data,
  staleTime: Infinity,
});

/** The chosen types with `type` added or removed, kept in the server's order. */
export function toggleType(
  chosen: readonly string[],
  type: string,
  on: boolean,
  order: readonly NotificationType[],
): string[] {
  const next = new Set(chosen);
  if (on) next.add(type);
  else next.delete(type);
  return order.map((t) => t.type).filter((t) => next.has(t));
}
