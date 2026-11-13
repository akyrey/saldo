// File downloads from the API (exports). Like every request, they go through
// the shared client, so the base URL and the CSRF header apply here too.
import { api } from "./client";
import { ApiError, apiErrorFrom } from "./errors";

export type ExportFormat = "json" | "csv";

export interface DownloadedFile {
  blob: Blob;
  filename: string;
}

const FALLBACK_NAMES: Record<ExportFormat, string> = {
  json: "saldo-export.json",
  csv: "saldo-export.zip",
};

/**
 * The file name in a `Content-Disposition: attachment; filename="..."`
 * header, reduced to its last path segment; `fallback` if there is none.
 */
export function filenameFrom(disposition: string | null, fallback: string): string {
  const match = disposition?.match(/filename="([^"]+)"/);
  const name = match?.[1]?.split(/[/\\]/).pop();
  return name ? name : fallback;
}

/** Downloads the household export (owner only) as a file in memory. */
export async function downloadExport(format: ExportFormat): Promise<DownloadedFile> {
  let result;
  try {
    result =
      format === "json"
        ? await api.GET("/api/v1/export/json", { parseAs: "blob" })
        : await api.GET("/api/v1/export/csv", { parseAs: "blob" });
  } catch (cause) {
    throw new ApiError(0, "network", cause instanceof Error ? cause.message : "network error");
  }
  const { response } = result;
  if (!response.ok || result.data === undefined) {
    throw apiErrorFrom(response.status, result.error);
  }
  const filename = filenameFrom(
    response.headers.get("Content-Disposition"),
    FALLBACK_NAMES[format],
  );
  return { blob: result.data as Blob, filename };
}
