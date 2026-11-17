import { describe, expect, it } from "vitest";
import { groupKey, plannedMerges, type DuplicateGroup } from "./duplicates";

const usage = (id: string, name: string, split_count: number) => ({ id, name, split_count });
const FOOD: DuplicateGroup = {
  tags: [usage("t1", "food", 9), usage("t2", "Food", 2), usage("t3", "FOOD", 0)],
};
const RENT: DuplicateGroup = { tags: [usage("t4", "Rent", 1), usage("t5", "rent", 0)] };

describe("plannedMerges", () => {
  it("keeps the most used tag of every group by default", () => {
    expect(plannedMerges([FOOD, RENT], {}, new Set())).toEqual([
      { into: "t1", sources: ["t2", "t3"] },
      { into: "t4", sources: ["t5"] },
    ]);
  });

  it("keeps the chosen tag and leaves out skipped groups", () => {
    const keep = { [groupKey(FOOD)]: "t2" };
    expect(plannedMerges([FOOD, RENT], keep, new Set([groupKey(RENT)]))).toEqual([
      { into: "t2", sources: ["t1", "t3"] },
    ]);
  });

  it("ignores a choice that is no longer in the group", () => {
    expect(plannedMerges([RENT], { [groupKey(RENT)]: "gone" }, new Set())).toEqual([
      { into: "t4", sources: ["t5"] },
    ]);
  });
});
