// File downloads from the API (exports, attachments). Like every request, they go through
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

/** What openapi-fetch resolves a blob download to. */
interface BlobResult {
  data?: unknown;
  error?: unknown;
  response: Response;
}

/** The body of a blob download, or an `ApiError` (`network` when offline). */
async function blobOf(
  request: () => Promise<BlobResult>,
): Promise<{ blob: Blob; response: Response }> {
  let result: BlobResult;
  try {
    result = await request();
  } catch (cause) {
    throw new ApiError(0, "network", cause instanceof Error ? cause.message : "network error");
  }
  const { response } = result;
  if (!response.ok || result.data === undefined) {
    throw apiErrorFrom(response.status, result.error);
  }
  return { blob: result.data as Blob, response };
}

/** Downloads the household export (owner only) as a file in memory. */
export async function downloadExport(format: ExportFormat): Promise<DownloadedFile> {
  const { blob, response } = await blobOf(() =>
    format === "json"
      ? api.GET("/api/v1/export/json", { parseAs: "blob" })
      : api.GET("/api/v1/export/csv", { parseAs: "blob" }),
  );
  const filename = filenameFrom(
    response.headers.get("Content-Disposition"),
    FALLBACK_NAMES[format],
  );
  return { blob, filename };
}

/**
 * Downloads an attachment. It keeps the name it was uploaded with: the
 * server's header replaces unusual characters for safety.
 */
export async function downloadAttachment(id: string, filename: string): Promise<DownloadedFile> {
  const { blob } = await blobOf(() =>
    api.GET("/api/v1/attachments/{id}/download", {
      params: { path: { id } },
      parseAs: "blob",
    }),
  );
  return { blob, filename };
}
