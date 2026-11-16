import { describe, expect, it } from "vitest";
import { activeFilterCount, parseTransactionSearch } from "./search";

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
