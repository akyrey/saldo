import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";

export type PiggyBank = components["schemas"]["PiggyBankView"];
export type PiggyBankEvent = components["schemas"]["PiggyBankEventView"];
export type NewPiggyBank = components["schemas"]["CreatePiggyBankRequest"];
export type NewPiggyBankEvent = components["schemas"]["PiggyBankEventRequest"];

/** Every piggy bank (the list page and the recurrence editor's picker). */
export const piggyBanksQuery = queryOptions({
  queryKey: ["piggy-banks"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(api.GET("/api/v1/piggy-banks", { params: { query: { page, per_page: MAX_PER_PAGE } } })),
    ),
});

export function createPiggyBank(body: NewPiggyBank) {
  return call(api.POST("/api/v1/piggy-banks", { body }));
}

export function addPiggyBankEvent(id: string, body: NewPiggyBankEvent) {
  return call(api.POST("/api/v1/piggy-banks/{id}/events", { params: { path: { id } }, body }));
}

export function deletePiggyBank(id: string) {
  return call(api.DELETE("/api/v1/piggy-banks/{id}", { params: { path: { id } } }));
}
