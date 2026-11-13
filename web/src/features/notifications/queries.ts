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
