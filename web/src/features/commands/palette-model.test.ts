import { describe, expect, it } from "vitest";
import { paletteGroups, searchTarget } from "./palette-model";

const EMPTY = { isOwner: false, templates: [], savedViews: [], accounts: [] };

describe("paletteGroups", () => {
  it("offers actions, pages and the Uncategorised view without any data", () => {
    const groups = paletteGroups(EMPTY);
    expect(groups.map((g) => g.heading)).toEqual(["Actions", "Pages", "Saved views"]);
    const labels = groups.flatMap((g) => g.items.map((i) => i.label));
    expect(labels).toContain("Budgets");
    expect(labels).toContain("Report: Cash flow");
    expect(labels).not.toContain("Webhooks");
  });

  it("adds owner pages, templates, saved views and accounts", () => {
    const groups = paletteGroups({
      isOwner: true,
      templates: [{ id: "t1", name: "Coffee" }],
      savedViews: [{ id: "v1", name: "Big", filters: { amount_min: "100" } }],
      accounts: [{ id: "a1", name: "Checking" }],
    });
    const items = groups.flatMap((g) => g.items);
    expect(items.find((i) => i.label === "Webhooks")).toBeDefined();
    expect(items.find((i) => i.id === "template:t1")?.target).toEqual({
      kind: "navigate",
      to: "/transactions/new",
      search: { template: "t1" },
    });
    expect(items.find((i) => i.id === "view:v1")?.target).toEqual({
      kind: "navigate",
      to: "/transactions",
      search: { amount_min: "100" },
    });
    expect(items.find((i) => i.id === "account:a1")?.target).toEqual({
      kind: "navigate",
      to: "/accounts/a1",
    });
  });

  it("gives every item a unique id", () => {
    const ids = paletteGroups({ ...EMPTY, isOwner: true }).flatMap((g) => g.items.map((i) => i.id));
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("searchTarget", () => {
  it("searches the transaction list for the trimmed text", () => {
    expect(searchTarget("  rent ")).toEqual({
      kind: "navigate",
      to: "/transactions",
      search: { q: "rent" },
    });
  });
});
