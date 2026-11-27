import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type SourceDocument = components["schemas"]["DocumentView"];
export type SourceRow = components["schemas"]["SourceRowView"];

export const sourceDocumentsQuery = queryOptions({
  queryKey: ["sources", "documents"],
  queryFn: async () => (await call(api.GET("/api/v1/source-documents"))).data,
});

/** The first page of stored rows, optionally only those not used yet. */
export function sourceRowsQuery(unmatched: boolean) {
  return queryOptions({
    queryKey: ["sources", "rows", unmatched],
    queryFn: async () =>
      await call(
        api.GET("/api/v1/source-rows", {
          params: { query: { unmatched, per_page: 100 } },
        }),
      ),
  });
}

/** Stores an Amazon or PayPal export. */
export async function uploadSource(kind: string, file: File): Promise<SourceDocument> {
  const form = new FormData();
  form.append("kind", kind);
  form.append("file", file, file.name);
  // openapi-fetch sends a FormData body as it is; the schema types it as an object.
  const body = form as unknown as components["schemas"]["SourceForm"];
  return (await call(api.POST("/api/v1/source-documents", { body }))).data;
}

export async function deleteSource(id: string): Promise<void> {
  await call(api.DELETE("/api/v1/source-documents/{id}", { params: { path: { id } } }));
}
