import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type Profile = components["schemas"]["ProfileView"];
export type ProfileRequest = components["schemas"]["ProfileRequest"];
export type Preset = components["schemas"]["Preset"];
export type Preview = components["schemas"]["PreviewView"];
export type ImportView = components["schemas"]["ImportView"];
export type ImportDetail = components["schemas"]["ImportDetail"];
export type Line = components["schemas"]["LineView"];
export type LinePatch = components["schemas"]["LinePatch"];

/** What a change to an import or profile can affect on screen. */
export const IMPORT_AFFECTS = [
  ["statement-imports"],
  ["transactions"],
  ["balances"],
  ["accounts"],
  ["reports"],
];

export const profilesQuery = queryOptions({
  queryKey: ["statement-profiles"],
  queryFn: async () => (await call(api.GET("/api/v1/statement-profiles"))).data,
});

export const presetsQuery = queryOptions({
  queryKey: ["statement-profiles", "presets"],
  queryFn: async () => (await call(api.GET("/api/v1/statement-profiles/presets"))).data,
  staleTime: Infinity,
});

/** An account's latest imports (one page is plenty to show). */
export function accountImportsQuery(accountId: string) {
  return queryOptions({
    queryKey: ["statement-imports", "account", accountId],
    queryFn: async () =>
      await call(
        api.GET("/api/v1/statement-imports", {
          params: { query: { account_id: accountId, per_page: 10 } },
        }),
      ),
  });
}

export function importQuery(id: string) {
  return queryOptions({
    queryKey: ["statement-imports", id],
    queryFn: async () =>
      (await call(api.GET("/api/v1/statement-imports/{id}", { params: { path: { id } } }))).data,
  });
}

/** Uploads a statement with a saved profile; the result is a draft to review. */
export async function uploadStatement(
  profileId: string,
  accountId: string,
  file: File,
): Promise<ImportDetail> {
  const form = new FormData();
  form.append("profile_id", profileId);
  form.append("account_id", accountId);
  form.append("file", file, file.name);
  // openapi-fetch sends a FormData body as it is (the browser adds the
  // multipart boundary); the schema types the form as an object instead.
  const body = form as unknown as components["schemas"]["UploadForm"];
  return (await call(api.POST("/api/v1/statement-imports", { body }))).data;
}

/** Reads a file with a mapping that is not saved; nothing is stored. */
export async function previewStatement(
  format: string,
  mapping: unknown,
  accountId: string | null,
  file: File,
): Promise<Preview> {
  const form = new FormData();
  form.append("format", format);
  form.append("mapping", JSON.stringify(mapping));
  if (accountId) form.append("account_id", accountId);
  form.append("file", file, file.name);
  const body = form as unknown as components["schemas"]["PreviewForm"];
  return (await call(api.POST("/api/v1/statement-profiles/preview", { body }))).data;
}

export async function updateLine(
  importId: string,
  position: number,
  patch: LinePatch,
): Promise<Line> {
  return (
    await call(
      api.PATCH("/api/v1/statement-imports/{id}/lines/{position}", {
        params: { path: { id: importId, position } },
        body: patch,
      }),
    )
  ).data;
}

type ImportAction = "commit" | "discard" | "revert";

/** Commits, discards or reverts an import. */
export async function importAction(id: string, action: ImportAction): Promise<void> {
  const params = { params: { path: { id } } };
  if (action === "commit") await call(api.POST("/api/v1/statement-imports/{id}/commit", params));
  else if (action === "discard")
    await call(api.POST("/api/v1/statement-imports/{id}/discard", params));
  else await call(api.POST("/api/v1/statement-imports/{id}/revert", params));
}
