import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { account, CURRENCIES, fakeApi, json, list, ME, MEMBERS } from "@/test/fake-api";
import { choose } from "@/test/combobox";
import { renderApp } from "@/test/render-app";

afterEach(() => vi.unstubAllGlobals());

const RENT = {
  id: "r1",
  title: "Rent",
  description: null,
  first_date: "2026-11-01",
  repeat_until: null,
  max_occurrences: null,
  apply_rules: true,
  active: true,
  notes: null,
  fired_through: "2026-11-13",
  last_fired_date: "2026-11-01",
  repetitions: [{ kind: "monthly", moment: "1", skip: 0, weekend_policy: "next_monday" }],
  splits: [
    {
      position: 0,
      description: "Rent",
      source_account_id: "a1",
      destination_account_id: "e1",
      amount: "800.00",
      currency: "EUR",
      tag_ids: [],
    },
  ],
  created_at: "",
  updated_at: "",
};

function api(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([
      account("a1", "Checking", "asset"),
      account("e1", "Landlord", "expense"),
    ]),
    "GET /api/v1/categories": list([]),
    "GET /api/v1/tags": list([]),
    "GET /api/v1/piggy-banks": list([]),
    "GET /api/v1/recurrences": list([RENT]),
    "GET /api/v1/recurrences/r1": { data: RENT },
    "POST /api/v1/recurrences/preview": { data: ["2026-12-01", "2027-01-01"] },
    ...extra,
  });
}

async function bodyOf(seen: Request[], method: string, path: string) {
  const request = seen.find((r) => r.method === method && new URL(r.url).pathname === path);
  return request ? ((await request.json()) as Record<string, unknown>) : undefined;
}

describe("RecurrencesPage", () => {
  it("folds inactive recurrences into a closed section and turns one off", async () => {
    const gym = { ...RENT, id: "r2", title: "Gym", active: false };
    const seen = api({
      "GET /api/v1/recurrences": list([gym, RENT]),
      "PUT /api/v1/recurrences/r1": { data: { ...RENT, active: false } },
    });
    renderApp("/recurrences");
    const rent = await screen.findByRole("listitem", { name: "Rent" });
    expect(rent.closest("details")).toBeNull();
    const section = screen.getByRole("listitem", { name: "Gym" }).closest("details");
    expect(section?.open).toBe(false);
    expect(within(section as HTMLElement).getByText("Inactive (1)")).toBeInTheDocument();

    await userEvent.click(within(rent).getByRole("switch", { name: "Rent active" }));
    await waitFor(() => expect(seen.some((r) => r.method === "PUT")).toBe(true));
    const body = await bodyOf(seen, "PUT", "/api/v1/recurrences/r1");
    expect(body).toMatchObject({ title: "Rent", active: false, apply_rules: true });
  });

  it("lists recurrences with their schedule and fires one now", async () => {
    const fired = { data: { recurrence_id: "r1", date: "2026-11-13", created: true, group: {} } };
    const seen = api({ "POST /api/v1/recurrences/r1/fire": json(201, fired) });
    renderApp("/recurrences");
    const item = await screen.findByRole("listitem", { name: "Rent" });
    expect(within(item).getByText("Monthly on day 1, weekends move to Monday")).toBeInTheDocument();
    expect(
      within(item).getByText(/Checking → Landlord · last created 2026-11-01/),
    ).toBeInTheDocument();

    await userEvent.click(within(item).getByRole("button", { name: /create today/i }));
    expect(await within(item).findByRole("status")).toHaveTextContent(
      "Created the transaction for 2026-11-13.",
    );
    expect(await bodyOf(seen, "POST", "/api/v1/recurrences/r1/fire")).toEqual({});
  });
});

describe("RecurrenceEditor", () => {
  it("previews upcoming dates and creates a recurrence from the template", async () => {
    const seen = api({ "POST /api/v1/recurrences": json(201, { data: RENT }) });
    renderApp("/recurrences/new");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Title"), "Gym");
    const repetition = screen.getByRole("group", { name: "Repetition 1" });
    await user.selectOptions(within(repetition).getByLabelText("Repeats"), "weekly");
    await user.selectOptions(within(repetition).getByLabelText("On"), "1");
    expect(within(repetition).getByText("Every Monday")).toBeInTheDocument();

    const preview = screen.getByRole("region", { name: "Upcoming dates" });
    expect(await within(preview).findByText("2026-12-01")).toBeInTheDocument();

    const split = screen.getByRole("group", { name: "Split 1" });
    expect(within(split).queryByLabelText("Date")).toBeNull();
    await user.type(within(split).getByLabelText("Description"), "Gym");
    await choose(within(split).getByLabelText("Source account"), "Checking (EUR)");
    await choose(within(split).getByLabelText("Destination account"), "Landlord (EUR)");
    await user.type(within(split).getByLabelText(/^Amount/), "30");
    await user.click(screen.getByRole("button", { name: "Create recurring transaction" }));

    await waitFor(() => expect(screen.queryByLabelText("Title")).toBeNull());
    const body = await bodyOf(seen, "POST", "/api/v1/recurrences");
    expect(body).toMatchObject({
      title: "Gym",
      repetitions: [{ kind: "weekly", moment: "1", skip: 0, weekend_policy: "keep" }],
      splits: [{ description: "Gym", source_id: "a1", destination_id: "e1", amount: "30" }],
    });
  });

  it("shows server errors and loads an existing recurrence for editing", async () => {
    const refused = json(422, {
      error: {
        code: "validation_failed",
        message: "invalid",
        details: [{ field: "repetitions[0].moment", code: "invalid", message: "bad moment" }],
      },
    });
    api({ "PUT /api/v1/recurrences/r1": refused });
    renderApp("/recurrences/r1/edit");
    expect(await screen.findByLabelText("Title")).toHaveValue("Rent");
    await userEvent.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByText("repetitions[0].moment: bad moment")).toBeInTheDocument();
  });
});
