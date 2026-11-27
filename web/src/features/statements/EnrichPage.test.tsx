// Enriching existing transactions against a fake API.
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const CANDIDATES = {
  data: [
    {
      split_id: "s1",
      group_id: "g1",
      date: "2026-11-05",
      amount: "-20.00",
      description: "AMAZON 405-1",
      proposal: {
        source: "amazon",
        description: "Amazon: Moka pot + 1 more",
        counterparty: "Amazon",
        notes: [],
        items: [
          { product: "Moka pot", amount_minor: 1500 },
          { product: "Filter", amount_minor: 500 },
        ],
      },
    },
    {
      split_id: "s2",
      group_id: "g2",
      date: "2026-11-06",
      amount: "-12.99",
      description: "PAYPAL EUROPE",
      proposal: {
        source: "paypal",
        description: "Netflix",
        counterparty: "Netflix",
        notes: [],
        items: [],
      },
    },
  ],
};

function api() {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts/a1": { data: account("a1", "Checking", "asset") },
    "GET /api/v1/accounts": list([account("a1", "Checking", "asset")]),
    "GET /api/v1/accounts/a1/enrichment": CANDIDATES,
    "POST /api/v1/accounts/a1/enrichment/accept": { data: [] },
  });
}

describe("enrich from sources", () => {
  it("accepts the selected transactions, one split by item", async () => {
    const seen = api();
    renderApp("/accounts/a1/enrich");
    const user = userEvent.setup();
    expect(await screen.findByText("Amazon: Moka pot + 1 more")).toBeInTheDocument();
    expect(screen.getByText("Netflix")).toBeInTheDocument();
    await user.click(screen.getByLabelText("Accept 2026-11-05 AMAZON 405-1"));
    await user.click(screen.getByLabelText("Split 2026-11-05 AMAZON 405-1 by item"));
    await user.click(screen.getByLabelText("Accept 2026-11-06 PAYPAL EUROPE"));
    await user.click(screen.getByRole("button", { name: "Accept 2 selected" }));
    await waitFor(() => expect(seen.some((r) => r.method === "POST")).toBe(true));
    const body = await seen.find((r) => r.method === "POST")?.json();
    expect(body).toEqual({
      items: [
        { split_id: "s1", split_by_item: true },
        { split_id: "s2", split_by_item: false },
      ],
    });
    const query = seen.find((r) => r.url.includes("/enrichment?"));
    expect(query?.url).toMatch(/from=\d{4}-\d{2}-\d{2}/);
  });

  it("forgets the selection when the dates change", async () => {
    api();
    renderApp("/accounts/a1/enrich");
    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("Accept 2026-11-06 PAYPAL EUROPE"));
    expect(screen.getByRole("button", { name: "Accept 1 selected" })).toBeEnabled();
    const from = screen.getByLabelText("From");
    // Type over the whole default date.
    await user.type(from, "2026-10-01", { initialSelectionStart: 0, initialSelectionEnd: 10 });
    expect(await screen.findByRole("button", { name: "Accept 0 selected" })).toBeDisabled();
  });
});
