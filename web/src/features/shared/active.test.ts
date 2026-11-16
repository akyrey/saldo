import { describe, expect, it } from "vitest";
import { partitionByActive } from "./active";

describe("partitionByActive", () => {
  it("splits items into active and inactive, keeping their order", () => {
    // Arrange
    const items = [
      { id: "a", active: false },
      { id: "b", active: true },
      { id: "c", active: false },
      { id: "d", active: true },
    ];

    // Act
    const { active, inactive } = partitionByActive(items);

    // Assert
    expect(active.map((i) => i.id)).toEqual(["b", "d"]);
    expect(inactive.map((i) => i.id)).toEqual(["a", "c"]);
  });

  it("returns two empty lists for no items", () => {
    expect(partitionByActive([])).toEqual({ active: [], inactive: [] });
  });
});
