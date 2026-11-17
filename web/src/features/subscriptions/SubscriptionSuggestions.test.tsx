// The bills page's suggestions against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const CANDIDATE = {
  account_id: "gym",
  account_name: "Gym",
  currency: "EUR",
  frequency: "monthly",
  amount_min: "30.00",
  amount_max: "30.00",
  first_payment: "2026-08-03",
  last_payment: "2026-11-03",
  next_due: "2026-12-03",
  split_ids: ["s1", "s2", "s3", "s4"],
  confidence: 100,
};

function api() {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/bills": list([]),
    "GET /api/v1/subscription-candidates": { data: [CANDIDATE] },
    "POST /api/v1/subscription-candidates/accept": { data: { id: "b1" } },
    "POST /api/v1/subscription-candidates/dismiss": {
      data: { account_id: "gym", currency: "EUR", created_at: "" },
    },
  });
}

describe("subscription suggestions", () => {
  it("adds a suggestion as a bill with an edited name", async () => {
    const seen = api();
    renderApp("/bills");
    const user = userEvent.setup();
    expect(await screen.findByText(/4 payments since 2026-08-03/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Add as bill" }));
    const form = screen.getByRole("form", { name: "Add Gym as a bill" });
    const name = within(form).getByLabelText("Bill name");
    await user.clear(name);
    await user.type(name, "Gym club");
    await user.click(within(form).getByRole("button", { name: "Create bill" }));

    await waitFor(() => expect(seen.some((r) => r.url.endsWith("/accept"))).toBe(true));
    const body = await seen.find((r) => r.url.endsWith("/accept"))?.json();
    expect(body).toEqual({
      account_id: "gym",
      currency: "EUR",
      name: "Gym club",
      frequency: "monthly",
      amount_min: "30.00",
      amount_max: "30.00",
      split_ids: ["s1", "s2", "s3", "s4"],
    });
  });

  it("dismisses a suggestion", async () => {
    const seen = api();
    renderApp("/bills");
    await userEvent.setup().click(await screen.findByRole("button", { name: "Dismiss Gym" }));
    await waitFor(() => expect(seen.some((r) => r.url.endsWith("/dismiss"))).toBe(true));
    const body = await seen.find((r) => r.url.endsWith("/dismiss"))?.json();
    expect(body).toEqual({ account_id: "gym", currency: "EUR" });
  });
});
