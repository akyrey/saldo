// Merging tags: the case-duplicates page and "Merge into…" on the tags page.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { accountRoutes, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

function loggedIn(extra: Record<string, unknown>) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    ...accountRoutes([], []),
    ...extra,
  });
}

const usage = (id: string, name: string, split_count: number) => ({ id, name, split_count });
const tag = (id: string, name: string) => ({ id, name, created_at: "", updated_at: "" });
const GROUPS = [
  { tags: [usage("t1", "food", 9), usage("t2", "Food", 2)] },
  { tags: [usage("t3", "Rent", 1), usage("t4", "rent", 0)] },
];

async function bodyOf(seen: Request[], method: string): Promise<unknown> {
  await waitFor(() => expect(seen.some((r) => r.method === method)).toBe(true));
  return seen.find((r) => r.method === method)?.json();
}

describe("case duplicates", () => {
  it("merges the chosen groups, keeping the chosen spelling", async () => {
    let groups = GROUPS;
    const seen = loggedIn({
      "GET /api/v1/tags/duplicates": () => ({ data: groups }),
      "POST /api/v1/tags/merge": () => {
        groups = [];
        return { data: [tag("t2", "Food")] };
      },
    });
    renderApp("/tags/duplicates");
    const user = userEvent.setup();

    const food = await screen.findByRole("group", { name: "food" });
    expect(within(food).getByRole("radio", { name: /^food/ })).toBeChecked();
    await user.click(within(food).getByRole("radio", { name: /^Food/ }));
    const rent = screen.getByRole("group", { name: "rent" });
    await user.click(within(rent).getByRole("checkbox", { name: "Merge these" }));
    await user.click(screen.getByRole("button", { name: "Merge 1 group" }));

    expect(await bodyOf(seen, "POST")).toEqual({ merges: [{ into: "t2", sources: ["t1"] }] });
    expect(await screen.findByText("Merged 1 group.")).toBeInTheDocument();
    expect(await screen.findByText("No tags differ only in case.")).toBeInTheDocument();
  });

  it("is reachable from the tags page", async () => {
    loggedIn({
      "GET /api/v1/tags": list([tag("t1", "food")]),
      "GET /api/v1/tags/duplicates": { data: [] },
    });
    renderApp("/tags");
    const user = userEvent.setup();

    await user.click(await screen.findByRole("link", { name: "Find case duplicates" }));
    expect(await screen.findByText("No tags differ only in case.")).toBeInTheDocument();
  });
});

describe("merge into", () => {
  it("folds one tag into another", async () => {
    const seen = loggedIn({
      "GET /api/v1/tags": list([tag("t1", "weekend"), tag("t2", "Weekends")]),
      "POST /api/v1/tags/merge": { data: [tag("t2", "Weekends")] },
    });
    renderApp("/tags");
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Merge weekend into…" }));
    await user.selectOptions(screen.getByLabelText("Merge weekend into"), "Weekends");
    await user.click(screen.getByRole("button", { name: "Merge" }));

    expect(await bodyOf(seen, "POST")).toEqual({ merges: [{ into: "t2", sources: ["t1"] }] });
  });
});
