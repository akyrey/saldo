import { describe, expect, it } from "vitest";
import { activeFilterCount, parseTransactionSearch, sameFilters, viewFilters } from "./search";

const ID = "0190f5c3-7a2b-7c00-8000-000000000001";

describe("parseTransactionSearch", () => {
  it("keeps valid filters", () => {
    const raw = {
      from: "2026-01-01",
      to: "2026-12-31",
      account_id: ID,
      category_id: ID,
      tag_id: ID,
      kind: "transfer",
      q: " rent ",
      page: 3,
    };
    expect(parseTransactionSearch(raw)).toEqual({ ...raw, q: "rent" });
  });

  it("drops malformed values", () => {
    const result = parseTransactionSearch({
      from: "yesterday",
      account_id: "1",
      kind: "theft",
      q: "   ",
      page: "1",
    });
    expect(Object.values(result).every((v) => v === undefined)).toBe(true);
  });
});

describe("activeFilterCount", () => {
  it("counts the folded filters, not the search text or the page", () => {
    expect(activeFilterCount({})).toBe(0);
    expect(activeFilterCount({ q: "bread", page: 2 })).toBe(0);
    expect(activeFilterCount({ from: "2026-01-01", kind: "deposit", account_id: ID })).toBe(3);
  });
});

describe("Phase 10 filters", () => {
  it("keeps none, amounts and flags", () => {
    expect(
      parseTransactionSearch({
        category_id: "none",
        budget_id: ID,
        bill_id: ID,
        reconciled: false,
        amount_min: "12,5",
        amount_max: "80",
        has_attachments: "true",
      }),
    ).toEqual({
      category_id: "none",
      budget_id: ID,
      bill_id: ID,
      reconciled: false,
      amount_min: "12.5",
      amount_max: "80",
      has_attachments: true,
    });
    expect(parseTransactionSearch({ budget_id: "none" }).budget_id).toBe("none");
  });

  it("drops malformed amounts and flags", () => {
    const result = parseTransactionSearch({
      amount_min: "-3",
      amount_max: "ten",
      reconciled: "maybe",
      has_attachments: 1,
      bill_id: "none",
    });
    expect(Object.values(result).every((v) => v === undefined)).toBe(true);
  });

  it("keeps amounts and search text the router parsed as numbers", () => {
    // A hand-typed `?amount_min=100&q=2024` reaches us as numbers, not strings.
    expect(parseTransactionSearch({ amount_min: 100, amount_max: 12.5, q: 2024 })).toMatchObject({
      amount_min: "100",
      amount_max: "12.5",
      q: "2024",
    });
    expect(parseTransactionSearch({ amount_min: -3, amount_max: Number.NaN }).amount_min).toBe(
      undefined,
    );
  });

  it("counts the new folded filters", () => {
    expect(activeFilterCount({ amount_min: "5", reconciled: true, budget_id: "none" })).toBe(3);
  });
});

describe("viewFilters and sameFilters", () => {
  it("stores only set filters, without the page", () => {
    expect(viewFilters({ q: "rent", page: 3, from: undefined })).toEqual({ q: "rent" });
  });

  it("compares filters ignoring the page and key order", () => {
    expect(sameFilters({ q: "a", kind: "deposit", page: 2 }, { kind: "deposit", q: "a" })).toBe(
      true,
    );
    expect(sameFilters({ q: "a" }, { q: "a", kind: "deposit" })).toBe(false);
    expect(sameFilters({}, {})).toBe(true);
  });
});
