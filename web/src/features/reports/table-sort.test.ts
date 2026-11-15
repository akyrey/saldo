import { describe, expect, it } from "vitest";
import { nextSort, sortedRows } from "./table-sort";

const LABELS = ["Food", "rent", "Car"];
const COLUMNS = [{ values: ["59.75", "900", "-12.5"] }, { values: ["0.00", "1.00", "2.00"] }];

describe("nextSort", () => {
  it("starts a new column ascending", () => {
    expect(nextSort(null, 1)).toEqual({ column: 1, direction: "asc" });
    expect(nextSort({ column: 0, direction: "desc" }, 1)).toEqual({ column: 1, direction: "asc" });
  });

  it("cycles the same column from ascending to descending to unsorted", () => {
    expect(nextSort({ column: 1, direction: "asc" }, 1)).toEqual({ column: 1, direction: "desc" });
    expect(nextSort({ column: 1, direction: "desc" }, 1)).toBeNull();
  });
});

describe("sortedRows", () => {
  it("keeps the server's order when unsorted", () => {
    expect(sortedRows(LABELS, COLUMNS, null)).toEqual([0, 1, 2]);
  });

  it("sorts labels alphabetically, ignoring case", () => {
    expect(sortedRows(LABELS, COLUMNS, { column: 0, direction: "asc" })).toEqual([2, 0, 1]);
    expect(sortedRows(LABELS, COLUMNS, { column: 0, direction: "desc" })).toEqual([1, 0, 2]);
  });

  it("sorts amounts exactly, whatever their number of decimals", () => {
    expect(sortedRows(LABELS, COLUMNS, { column: 1, direction: "asc" })).toEqual([2, 0, 1]);
    expect(sortedRows(LABELS, COLUMNS, { column: 1, direction: "desc" })).toEqual([1, 0, 2]);
  });

  it("sorts amounts beyond float precision correctly", () => {
    const big = [{ values: ["9007199254740993.01", "9007199254740993.00"] }];
    expect(sortedRows(["a", "b"], big, { column: 1, direction: "asc" })).toEqual([1, 0]);
  });

  it("keeps equal values in the server's order", () => {
    const ties = [{ values: ["1.00", "1.00", "0.50"] }];
    expect(sortedRows(["a", "b", "c"], ties, { column: 1, direction: "desc" })).toEqual([0, 1, 2]);
  });

  it("treats missing or invalid amounts as zero", () => {
    const odd = [{ values: ["oops", "-1.00"] }];
    expect(sortedRows(["a", "b", "c"], odd, { column: 1, direction: "asc" })).toEqual([1, 0, 2]);
  });
});
