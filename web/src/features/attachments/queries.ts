import { queryOptions } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import type { components } from "@/api/schema";

export type Attachment = components["schemas"]["AttachmentView"];

/** What an attachment can belong to in the web app. */
export type AttachableType = "split" | "bill";

export interface AttachmentTarget {
  type: AttachableType;
  id: string;
}

/** Largest file the server accepts (`saldo_app::attachments::MAX_SIZE`). */
export const MAX_ATTACHMENT_BYTES = 20 * 1024 * 1024;

/**
 * File types the server accepts, for the file picker's `accept`. Extensions
 * as well as MIME types, since some browsers report no type (HEIC photos).
 */
export const ACCEPTED_FILES = [
  "application/pdf",
  "image/*",
  "text/plain",
  "text/csv",
  "application/xml",
  "text/xml",
  ".pdf",
  ".png",
  ".jpg",
  ".jpeg",
  ".gif",
  ".webp",
  ".heic",
  ".txt",
  ".csv",
  ".xml",
].join(",");

export function attachmentsQuery(target: AttachmentTarget) {
  return queryOptions({
    queryKey: ["attachments", target.type, target.id],
    queryFn: async () =>
      (
        await call(
          api.GET("/api/v1/attachments", {
            params: { query: { attachable_type: target.type, attachable_id: target.id } },
          }),
        )
      ).data,
  });
}

/** Uploads one file; the title is optional. */
export async function uploadAttachment(
  target: AttachmentTarget,
  file: File,
  title: string,
): Promise<Attachment> {
  const form = new FormData();
  form.append("attachable_type", target.type);
  form.append("attachable_id", target.id);
  if (title.trim() !== "") form.append("title", title.trim());
  form.append("file", file, file.name);
  // openapi-fetch sends a FormData body as it is (the browser adds the
  // multipart boundary); the schema types the form as an object instead.
  const body = form as unknown as components["schemas"]["UploadForm"];
  return (await call(api.POST("/api/v1/attachments", { body }))).data;
}

export async function deleteAttachment(id: string): Promise<void> {
  await call(api.DELETE("/api/v1/attachments/{id}", { params: { path: { id } } }));
}
