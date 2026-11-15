// The attachments panel against a fake API.
import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createQueryClient } from "@/lib/query-client";
import { apiError, fakeApi } from "@/test/fake-api";
import { AttachmentsPanel } from "./AttachmentsPanel";

afterEach(() => {
  vi.unstubAllGlobals();
});

function attachment(id: string, filename: string, title: string | null = null) {
  return {
    id,
    attachable_type: "split",
    attachable_id: "s1",
    filename,
    title,
    mime_type: "application/pdf",
    size_bytes: 2048,
    sha256: "abc",
    notes: null,
    created_at: "",
  };
}

function renderPanel() {
  render(
    <QueryClientProvider client={createQueryClient()}>
      <AttachmentsPanel target={{ type: "split", id: "s1" }} />
    </QueryClientProvider>,
  );
}

const path = (request: Request) => `${request.method} ${new URL(request.url).pathname}`;

describe("AttachmentsPanel", () => {
  it("lists the target's files with their size", async () => {
    const seen = fakeApi({
      "GET /api/v1/attachments": { data: [attachment("f1", "receipt.pdf", "Receipt")] },
    });
    renderPanel();
    expect(await screen.findByText("Receipt")).toBeInTheDocument();
    expect(screen.getByText("receipt.pdf · 2.0 KB")).toBeInTheDocument();
    const query = new URL(seen[0]?.url ?? "http://x").searchParams;
    expect([query.get("attachable_type"), query.get("attachable_id")]).toEqual(["split", "s1"]);
  });

  it("says when there are no files", async () => {
    fakeApi({ "GET /api/v1/attachments": { data: [] } });
    renderPanel();
    expect(await screen.findByText("No files yet.")).toBeInTheDocument();
  });

  it("uploads a file with a title as a multipart form", async () => {
    let files = [] as ReturnType<typeof attachment>[];
    const seen = fakeApi({
      "GET /api/v1/attachments": () => ({ data: files }),
      "POST /api/v1/attachments": () => {
        files = [attachment("f2", "bill.pdf", "Invoice")];
        return new Response(JSON.stringify({ data: files[0] }), {
          status: 201,
          headers: { "Content-Type": "application/json" },
        });
      },
    });
    renderPanel();
    await screen.findByText("No files yet.");
    const file = new File(["%PDF"], "bill.pdf", { type: "application/pdf" });
    await userEvent.upload(screen.getByLabelText("File"), file);
    await userEvent.type(screen.getByLabelText("Title (optional)"), "Invoice");
    await userEvent.click(screen.getByRole("button", { name: "Upload" }));

    expect(await screen.findByText("Invoice")).toBeInTheDocument();
    const post = seen.find((r) => path(r) === "POST /api/v1/attachments");
    const form = await post?.formData();
    expect(form?.get("attachable_type")).toBe("split");
    expect(form?.get("attachable_id")).toBe("s1");
    expect(form?.get("title")).toBe("Invoice");
    // jsdom's File reaches the fake server as a plain Blob, so check the content.
    expect(await (form?.get("file") as Blob).text()).toBe("%PDF");
    expect(screen.getByLabelText("Title (optional)")).toHaveValue("");
  });

  it("refuses files over 20 MB before sending them", async () => {
    const seen = fakeApi({ "GET /api/v1/attachments": { data: [] } });
    renderPanel();
    await screen.findByText("No files yet.");
    const big = new File(["x"], "scan.pdf", { type: "application/pdf" });
    Object.defineProperty(big, "size", { value: 21 * 1024 * 1024 });
    await userEvent.upload(screen.getByLabelText("File"), big);
    expect(screen.getByText("This file is larger than 20 MB.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Upload" })).toBeDisabled();
    expect(seen.some((r) => r.method === "POST")).toBe(false);
  });

  it("shows the server's reason when an upload is refused", async () => {
    fakeApi({
      "GET /api/v1/attachments": { data: [] },
      "POST /api/v1/attachments": apiError(422, "validation", "not an accepted file type"),
    });
    renderPanel();
    await screen.findByText("No files yet.");
    await userEvent.upload(screen.getByLabelText("File"), new File(["x"], "a.pdf"));
    await userEvent.click(screen.getByRole("button", { name: "Upload" }));
    expect(await screen.findByText(/not an accepted file type/)).toBeInTheDocument();
  });

  it("deletes a file after a second click", async () => {
    let files = [attachment("f1", "receipt.pdf")];
    const seen = fakeApi({
      "GET /api/v1/attachments": () => ({ data: files }),
      "DELETE /api/v1/attachments/f1": () => {
        files = [];
        return new Response(null, { status: 204 });
      },
    });
    renderPanel();
    const row = (await screen.findByText("receipt.pdf")).closest("li") as HTMLElement;
    await userEvent.click(within(row).getByRole("button", { name: "Delete receipt.pdf" }));
    expect(seen.some((r) => r.method === "DELETE")).toBe(false);
    await userEvent.click(within(row).getByRole("button", { name: "Really delete receipt.pdf?" }));
    expect(await screen.findByText("No files yet.")).toBeInTheDocument();
  });

  it("downloads a file under its own name", async () => {
    const saved: string[] = [];
    vi.stubGlobal(
      "URL",
      Object.assign(URL, {
        createObjectURL: () => "blob:x",
        revokeObjectURL: () => {},
      }),
    );
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      saved.push(this.download);
    });
    fakeApi({
      "GET /api/v1/attachments": { data: [attachment("f1", "receipt.pdf")] },
      "GET /api/v1/attachments/f1/download": () => new Response("%PDF", { status: 200 }),
    });
    renderPanel();
    await userEvent.click(await screen.findByRole("button", { name: "Download receipt.pdf" }));
    await waitFor(() => expect(saved).toEqual(["receipt.pdf"]));
    click.mockRestore();
  });
});
