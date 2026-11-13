import { describe, expect, it } from "vitest";
import { filterOptions } from "./combobox-filter";

const OPTIONS = [
  { value: "1", label: "Groceries", hint: "Expense" },
  { value: "2", label: "Car insurance", hint: "Expense" },
  { value: "3", label: "Checking (EUR)", hint: "Asset" },
  { value: "4", label: "Cash (EUR)", hint: "Asset" },
];

describe("filterOptions", () => {
  it("keeps everything for empty text", () => {
    expect(filterOptions(OPTIONS, "  ")).toHaveLength(4);
  });

  it("puts names starting with the text first, ignoring case", () => {
    expect(filterOptions(OPTIONS, "CA").map((o) => o.value)).toEqual(["2", "4"]);
    expect(filterOptions(OPTIONS, "ur").map((o) => o.value)).toEqual(["2", "3", "4"]);
  });

  it("searches the hint too", () => {
    expect(filterOptions(OPTIONS, "asset").map((o) => o.value)).toEqual(["3", "4"]);
  });
});
