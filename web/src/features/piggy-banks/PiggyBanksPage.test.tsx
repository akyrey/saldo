import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, apiError, CURRENCIES, fakeApi, json, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => vi.unstubAllGlobals());

const HOLIDAY = {
  id: "p1",
  name: "Holiday",
  currency: "EUR",
  target_amount: "300.00",
  saved_amount: "100.00",
  left_to_save: "200.00",
  start_date: null,
  target_date: "2027-06-30",
  active: true,
  sort_order: 0,
  notes: null,
  accounts: [{ account_id: "a1", saved_amount: "100.00" }],
  created_at: "",
  updated_at: "",
};

function api(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([
      account("a1", "Savings", "asset"),
      account("a2", "Checking", "asset"),
      account("x", "Shop", "expense"),
    ]),
    "GET /api/v1/piggy-banks": list([HOLIDAY]),
    ...extra,
  });
}

async function bodyOf(seen: Request[], method: string, path: string) {
  const request = seen.find((r) => r.method === method && new URL(r.url).pathname === path);
  return request ? ((await request.json()) as Record<string, unknown>) : undefined;
}

describe("PiggyBanksPage", () => {
  it("shows progress towards the target and the linked accounts", async () => {
    api();
    renderApp("/piggy-banks");
    const card = await screen.findByRole("listitem", { name: "Holiday" });
    expect(within(card).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "33");
    expect(within(card).getByText(/33% · by 2027-06-30 · in Savings/)).toBeInTheDocument();
  });

  it("removes money with a negative amount", async () => {
    const seen = api({ "POST /api/v1/piggy-banks/p1/events": json(201, { data: {} }) });
    renderApp("/piggy-banks");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /remove money/i }));
    const dialog = screen.getByRole("dialog", { name: "Remove money from Holiday" });
    await user.type(within(dialog).getByLabelText(/amount/i), "12,50");
    await user.click(within(dialog).getByRole("button", { name: "Remove" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const body = await bodyOf(seen, "POST", "/api/v1/piggy-banks/p1/events");
    expect(body).toMatchObject({ account_id: "a1", amount: "-12.50" });
  });

  it("shows the server's reason when money cannot be added", async () => {
    const refused = json(422, {
      error: {
        code: "validation_failed",
        message: "invalid",
        details: [{ field: "amount", code: "invalid", message: "goes past the target" }],
      },
    });
    api({ "POST /api/v1/piggy-banks/p1/events": refused });
    renderApp("/piggy-banks");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /add money/i }));
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText(/amount/i), "999");
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(await within(dialog).findByText("goes past the target")).toBeInTheDocument();
  });

  it("creates a piggy bank on the chosen asset accounts", async () => {
    const seen = api({ "POST /api/v1/piggy-banks": json(201, { data: HOLIDAY }) });
    renderApp("/piggy-banks");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /new piggy bank/i }));
    const form = screen.getByRole("form", { name: "New piggy bank" });
    expect(within(form).queryByLabelText("Shop")).toBeNull();
    await user.type(within(form).getByLabelText("Name"), "Car");
    await user.type(within(form).getByLabelText(/^Target \(/), "5000");
    await user.click(within(form).getByLabelText("Checking"));
    await user.click(within(form).getByRole("button", { name: "Create piggy bank" }));

    await waitFor(() => expect(screen.queryByRole("form", { name: "New piggy bank" })).toBeNull());
    const body = await bodyOf(seen, "POST", "/api/v1/piggy-banks");
    expect(body).toMatchObject({ name: "Car", target_amount: "5000", account_ids: ["a1", "a2"] });
  });

  it("reports a failed list", async () => {
    api({ "GET /api/v1/piggy-banks": apiError(403, "forbidden", "boom") });
    renderApp("/piggy-banks");
    expect(await screen.findByText("boom")).toBeInTheDocument();
  });
});
