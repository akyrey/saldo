import { describe, expect, it } from "vitest";
import { moveChanges } from "./template-model";

const items = (orders: number[]) => orders.map((sort_order, i) => ({ id: `t${i}`, sort_order }));

describe("moveChanges", () => {
  it("swaps neighbours when the stored orders are already positions", () => {
    expect(moveChanges(items([0, 1, 2]), 2, -1)).toEqual([
      { id: "t2", sort_order: 1 },
      { id: "t1", sort_order: 2 },
    ]);
  });

  it("renumbers every item whose stored order differs from its new position", () => {
    expect(moveChanges(items([5, 5, 9]), 0, 1)).toEqual([
      { id: "t1", sort_order: 0 },
      { id: "t0", sort_order: 1 },
      { id: "t2", sort_order: 2 },
    ]);
  });

  it("does nothing past either end", () => {
    expect(moveChanges(items([0, 1]), 0, -1)).toEqual([]);
    expect(moveChanges(items([0, 1]), 1, 1)).toEqual([]);
  });
});
