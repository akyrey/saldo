// Statement profiles against a fake API: create from a preset, preview, delete.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const SATISPAY_MAPPING = {
  date_column: ["Date"],
  amount: { kind: "signed", column: ["Balance"] },
  decimal_separator: ".",
  description_columns: [["Name"]],
  part_columns: [["Meal Vouchers"], ["Shopping Vouchers"]],
};

const PRESETS = {
  data: [
    { name: "Widiba", format: "xlsx", mapping: { date_column: ["DATA CONT."] } },
    { name: "Satispay", format: "xlsx", mapping: SATISPAY_MAPPING },
  ],
};

const SAVED = {
  id: "p1",
  name: "Satispay Dario",
  account_id: "a2",
  format: "xlsx",
  mapping: SATISPAY_MAPPING,
  part_accounts: { "Meal Vouchers": "a3" },
  transfer_rules: [],
  created_at: "",
  updated_at: "",
};

function api(profiles: object[] = []) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([
      account("a1", "Widiba", "asset"),
      account("a2", "Satispay", "asset"),
      account("a3", "Meal vouchers", "asset"),
      account("x1", "Groceries", "expense"),
    ]),
    "GET /api/v1/statement-profiles": { data: profiles },
    "GET /api/v1/statement-profiles/presets": PRESETS,
    "POST /api/v1/statement-profiles": { data: SAVED },
    "DELETE /api/v1/statement-profiles/p1": new Response(null, { status: 204 }),
    "POST /api/v1/statement-profiles/preview": {
      data: {
        currency: "EUR",
        lines: [
          {
            row: 2,
            accounting_date: "2025-08-05",
            value_date: "2025-08-05",
            booked_date: "2025-08-05",
            amount: "-5.00",
            description: "Merchant 2",
            category: null,
            counterparty: "Merchant 2",
            external_id: null,
            notes: [],
            parts: ["Meal Vouchers: -8.00"],
          },
        ],
        total_lines: 3,
        unsupported: [{ row: 9, reason: "cancelled" }],
        opening_balance: "129.16",
        closing_balance: "108.16",
      },
    },
  });
}

describe("statement profiles", () => {
  it("creates a profile from a preset with the meal-voucher account", async () => {
    const seen = api();
    renderApp("/statement-profiles");
    const user = userEvent.setup();
    // The presets arrive after the form renders.
    const start = await screen.findByLabelText("Start from");
    await within(start).findByRole("option", { name: "Satispay" });
    await user.selectOptions(start, "Satispay");
    // The preset filled in its own name; replace it.
    expect(screen.getByLabelText("Name")).toHaveValue("Satispay");
    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "Satispay Dario");
    await user.selectOptions(screen.getByLabelText("Account"), "a2");
    await user.selectOptions(screen.getByLabelText("Meal Vouchers"), "a3");
    await user.click(screen.getByRole("button", { name: "Save profile" }));

    await waitFor(() => expect(seen.some((r) => r.method === "POST")).toBe(true));
    // The form starts over, so a second click cannot create a duplicate.
    await waitFor(() => expect(screen.getByLabelText("Name")).toHaveValue(""));
    const body = await seen.find((r) => r.method === "POST")?.json();
    expect(body).toEqual({
      name: "Satispay Dario",
      account_id: "a2",
      format: "xlsx",
      mapping: SATISPAY_MAPPING,
      part_accounts: { "Meal Vouchers": "a3", "Shopping Vouchers": null },
      transfer_rules: [],
    });
  });

  it("previews a file with the mapping before saving", async () => {
    api();
    renderApp("/statement-profiles");
    const user = userEvent.setup();
    // The presets arrive after the form renders.
    const start = await screen.findByLabelText("Start from");
    await within(start).findByRole("option", { name: "Satispay" });
    await user.selectOptions(start, "Satispay");
    await user.upload(screen.getByLabelText("Sample file"), new File(["x"], "satispay.xlsx"));
    await user.click(screen.getByRole("button", { name: "Preview" }));
    expect(await screen.findByText("Merchant 2")).toBeInTheDocument();
    expect(screen.getByText("Meal Vouchers: -8.00")).toBeInTheDocument();
    expect(screen.getByText(/3 lines/)).toBeInTheDocument();
    expect(screen.getByText(/Row 9: cancelled/)).toBeInTheDocument();
  });

  it("refuses mapping JSON that does not parse", async () => {
    const seen = api();
    renderApp("/statement-profiles");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Name"), "Broken");
    const mapping = screen.getByLabelText("Mapping (JSON)");
    await user.clear(mapping);
    await user.type(mapping, "{{not json");
    await user.click(screen.getByRole("button", { name: "Save profile" }));
    expect(await screen.findByText(/not a valid JSON object/)).toBeInTheDocument();
    expect(seen.some((r) => r.method === "POST")).toBe(false);
  });

  it("deletes a profile after asking", async () => {
    const seen = api([SAVED]);
    renderApp("/statement-profiles");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Delete Satispay Dario" }));
    await user.click(screen.getByRole("button", { name: "Really delete?" }));
    await waitFor(() => expect(seen.some((r) => r.method === "DELETE")).toBe(true));
  });
});
