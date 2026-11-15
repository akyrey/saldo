import { describe, expect, it } from "vitest";
import { split } from "@/test/fake-api";
import { descriptionSuggestions } from "./description-suggestions";
import type { TransactionGroup } from "./queries";

function group(title: string | null, ...descriptions: string[]): TransactionGroup {
  return {
    id: descriptions.join(),
    title,
    created_at: "",
    updated_at: "",
    splits: descriptions.map((d, i) => ({ ...split(`s${i}`, "a1", "a2", "1.00"), description: d })),
  } as TransactionGroup;
}

describe("descriptionSuggestions", () => {
  it("puts prefix matches first, then the most used", () => {
    const groups = [
      group(null, "Weekly groceries"),
      group(null, "Groceries"),
      group(null, "Weekly groceries"),
      group(null, "Groceries Lidl"),
    ];
    expect(descriptionSuggestions(groups, "groc", 10)).toEqual([
      "Groceries",
      "Groceries Lidl",
      "Weekly groceries",
    ]);
  });

  it("merges spellings that differ in case, keeping the newest one", () => {
    const groups = [group(null, "Rent"), group(null, "rent")];
    expect(descriptionSuggestions(groups, "re", 10)).toEqual(["Rent"]);
  });

  it("includes group titles and leaves out what is already typed", () => {
    const groups = [group("Weekend trip", "Fuel", "Hotel"), group(null, "Weekend")];
    expect(descriptionSuggestions(groups, "weekend", 10)).toEqual(["Weekend trip"]);
  });

  it("ignores blank text and respects the limit", () => {
    const groups = [group(null, "a1"), group(null, "a2"), group(null, "a3")];
    expect(descriptionSuggestions(groups, "  ", 10)).toEqual([]);
    expect(descriptionSuggestions(groups, "a", 2)).toHaveLength(2);
  });
});
