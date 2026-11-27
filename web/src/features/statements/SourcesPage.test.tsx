// Amazon and PayPal exports against a fake API: upload, list, delete.
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const DOCUMENT = {
  id: "d1",
  kind: "amazon_orders",
  filename: "orders.csv",
  first_date: "2020-01-26",
  last_date: "2022-11-30",
  row_count: 2,
  rows_added: 2,
  rows_updated: 0,
  created_at: "2026-11-27T10:00:00Z",
};

const ROW = {
  id: "r1",
  kind: "amazon_orders",
  external_id: "171-1:B1:1",
  date: "2020-01-26",
  time: null,
  amount: "103.89",
  currency: "EUR",
  row_kind: "item",
  group_key: "171-1",
  counterparty: null,
  text: "Bialetti Mokona",
  status: "Closed",
  excluded: false,
  matched: false,
};

function api() {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/source-documents": { data: [DOCUMENT] },
    "GET /api/v1/source-rows": list([ROW]),
    "POST /api/v1/source-documents": {
      data: { ...DOCUMENT, id: "d2", rows_added: 1, rows_updated: 3 },
    },
    "DELETE /api/v1/source-documents/d1": new Response(null, { status: 204 }),
  });
}

describe("sources", () => {
  it("uploads an export and says what it added", async () => {
    const seen = api();
    renderApp("/sources");
    const user = userEvent.setup();
    await user.selectOptions(await screen.findByLabelText("Export"), "paypal");
    await user.upload(screen.getByLabelText("File"), new File(["x"], "paypal.csv"));
    await user.click(screen.getByRole("button", { name: "Upload" }));
    expect(await screen.findByText("1 new rows, 3 updated.")).toBeInTheDocument();
    const form = await seen.find((r) => r.method === "POST")?.formData();
    expect(form?.get("kind")).toBe("paypal");
  });

  it("lists uploads and rows, and deletes an upload after asking", async () => {
    const seen = api();
    renderApp("/sources");
    const user = userEvent.setup();
    expect(await screen.findByText(/orders\.csv/)).toBeInTheDocument();
    expect(screen.getByText("2020-01-26 to 2022-11-30 · 2 rows")).toBeInTheDocument();
    expect(await screen.findByText("Bialetti Mokona")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Delete orders.csv" }));
    await user.click(screen.getByRole("button", { name: "Really delete?" }));
    await waitFor(() => expect(seen.some((r) => r.method === "DELETE")).toBe(true));
  });

  it("shows only unused rows when asked", async () => {
    const seen = api();
    renderApp("/sources");
    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("Only rows not used yet"));
    await waitFor(() =>
      expect(
        seen.some((r) => r.url.includes("/source-rows") && r.url.includes("unmatched=true")),
      ).toBe(true),
    );
  });
});
