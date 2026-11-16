import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";
import { fetchAllPages, MAX_PER_PAGE } from "@/features/shared/pagination";

export type Recurrence = components["schemas"]["RecurrenceView"];
export type Repetition = components["schemas"]["RepetitionView"];
export type RecurrenceRequest = components["schemas"]["RecurrenceRequest"];
export type PreviewRequest = components["schemas"]["PreviewRequest"];

export const recurrencesQuery = queryOptions({
  queryKey: ["recurrences"],
  queryFn: () =>
    fetchAllPages((page) =>
      call(api.GET("/api/v1/recurrences", { params: { query: { page, per_page: MAX_PER_PAGE } } })),
    ),
});

export function recurrenceQuery(id: string) {
  return queryOptions({
    queryKey: ["recurrences", "one", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/recurrences/{id}", { params: { path: { id } } }))).data,
  });
}

/** Upcoming dates of an unsaved schedule; the key is the schedule itself. */
export function previewQuery(body: PreviewRequest) {
  return queryOptions({
    queryKey: ["recurrences", "preview", body],
    queryFn: async () => (await call(api.POST("/api/v1/recurrences/preview", { body }))).data,
    retry: false,
  });
}

/** Creates today's (or `date`'s) transaction; firing twice is harmless. */
export function fireRecurrence(id: string, date?: string) {
  return call(
    api.POST("/api/v1/recurrences/{id}/fire", { params: { path: { id } }, body: { date } }),
  );
}

/** Replaces a recurrence; the list uses it to switch one on or off. */
export function replaceRecurrence(id: string, body: RecurrenceRequest) {
  return call(api.PUT("/api/v1/recurrences/{id}", { params: { path: { id } }, body }));
}
