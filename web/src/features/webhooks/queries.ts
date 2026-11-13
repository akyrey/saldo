import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type Webhook = components["schemas"]["WebhookView"];
export type WebhookWithSecret = components["schemas"]["WebhookWithSecret"];
export type Delivery = components["schemas"]["DeliveryView"];

export interface WebhookInput {
  name: string;
  url: string;
  event_types: string[];
  include_import_events: boolean;
}

export const DELIVERIES_PER_PAGE = 20;

export const webhooksQuery = queryOptions({
  queryKey: ["webhooks", "all"],
  queryFn: async (): Promise<Webhook[]> => (await call(api.GET("/api/v1/webhooks"))).data,
});

export function deliveriesQuery(id: string, page: number) {
  return queryOptions({
    queryKey: ["webhooks", "deliveries", id, page],
    queryFn: () =>
      call(
        api.GET("/api/v1/webhooks/{id}/deliveries", {
          params: { path: { id }, query: { page, per_page: DELIVERIES_PER_PAGE } },
        }),
      ),
  });
}

export async function createWebhook(input: WebhookInput): Promise<WebhookWithSecret> {
  return (await call(api.POST("/api/v1/webhooks", { body: input }))).data;
}

export async function updateWebhook(
  id: string,
  patch: Partial<WebhookInput> & { active?: boolean },
): Promise<Webhook> {
  return (await call(api.PATCH("/api/v1/webhooks/{id}", { params: { path: { id } }, body: patch })))
    .data;
}

export async function deleteWebhook(id: string): Promise<void> {
  await call(api.DELETE("/api/v1/webhooks/{id}", { params: { path: { id } } }));
}

export async function testWebhook(id: string): Promise<void> {
  await call(api.POST("/api/v1/webhooks/{id}/test", { params: { path: { id } } }));
}

export async function rotateSecret(id: string): Promise<WebhookWithSecret> {
  return (await call(api.POST("/api/v1/webhooks/{id}/rotate-secret", { params: { path: { id } } })))
    .data;
}

export async function redeliver(id: string): Promise<Delivery> {
  return (
    await call(api.POST("/api/v1/webhook-deliveries/{id}/redeliver", { params: { path: { id } } }))
  ).data;
}
