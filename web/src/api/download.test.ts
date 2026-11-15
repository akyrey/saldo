import { afterEach, describe, expect, it, vi } from "vitest";
import { apiError, fakeApi } from "@/test/fake-api";
import { downloadAttachment, downloadExport, filenameFrom } from "./download";
import { ApiError } from "./errors";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("filenameFrom", () => {
  it("reads the quoted file name of a Content-Disposition header", () => {
    expect(filenameFrom('attachment; filename="saldo-export-2026-11-13.zip"', "x")).toBe(
      "saldo-export-2026-11-13.zip",
    );
  });

  it("falls back when the header is missing or has no file name", () => {
    expect(filenameFrom(null, "saldo-export.json")).toBe("saldo-export.json");
    expect(filenameFrom("attachment", "saldo-export.json")).toBe("saldo-export.json");
  });

  it("never returns a path", () => {
    expect(filenameFrom('attachment; filename="../../etc/passwd"', "x")).toBe("passwd");
  });
});

describe("downloadExport", () => {
  it("returns the file and its server-chosen name", async () => {
    fakeApi({
      "GET /api/v1/export/csv": () =>
        new Response("zip-bytes", {
          status: 200,
          headers: {
            "Content-Type": "application/zip",
            "Content-Disposition": 'attachment; filename="saldo-export-2026-11-13.zip"',
          },
        }),
    });
    const file = await downloadExport("csv");
    expect(file.filename).toBe("saldo-export-2026-11-13.zip");
    expect(await file.blob.text()).toBe("zip-bytes");
  });

  it("turns an error envelope into an ApiError", async () => {
    fakeApi({ "GET /api/v1/export/json": apiError(403, "forbidden", "owners only") });
    await expect(downloadExport("json")).rejects.toEqual(
      new ApiError(403, "forbidden", "owners only"),
    );
  });
});

describe("downloadAttachment", () => {
  it("returns the file under its stored name", async () => {
    fakeApi({
      "GET /api/v1/attachments/f1/download": () =>
        new Response("%PDF", {
          status: 200,
          headers: { "Content-Disposition": 'attachment; filename="receipt_1.pdf"' },
        }),
    });
    const file = await downloadAttachment("f1", "receipt#1.pdf");
    expect(file.filename).toBe("receipt#1.pdf");
    expect(await file.blob.text()).toBe("%PDF");
  });

  it("turns a missing attachment into an ApiError", async () => {
    fakeApi({});
    await expect(downloadAttachment("gone", "x.pdf")).rejects.toBeInstanceOf(ApiError);
  });
});
