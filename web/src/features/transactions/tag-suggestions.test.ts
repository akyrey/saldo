import { describe, expect, it } from "vitest";
import { tagSuggestions } from "./tag-suggestions";

const tag = (id: string, name: string) => ({ id, name, created_at: "", updated_at: "" });
const TAGS = [
  tag("1", "groceries"),
  tag("2", "Holiday 2026"),
  tag("3", "holiday-gifts"),
  tag("4", "car"),
  tag("5", "Car insurance"),
];

describe("tagSuggestions", () => {
  it("matches anywhere, ignoring case, names starting with the text first", () => {
    const { matches } = tagSuggestions(TAGS, [], "car", 8);
    expect(matches.map((t) => t.name)).toEqual(["car", "Car insurance"]);
    expect(tagSuggestions(TAGS, [], "IDAY", 8).matches.map((t) => t.id)).toEqual(["2", "3"]);
  });

  it("leaves out chosen tags and stops at the limit", () => {
    expect(tagSuggestions(TAGS, ["4"], "car", 8).matches.map((t) => t.id)).toEqual(["5"]);
    expect(tagSuggestions(TAGS, [], "o", 2).matches).toHaveLength(2);
  });

  it("offers to create a tag only when no tag has exactly that name", () => {
    expect(tagSuggestions(TAGS, [], "  Car ", 8).create).toBeNull();
    expect(tagSuggestions(TAGS, [], "carpool", 8).create).toBe("carpool");
    expect(tagSuggestions(TAGS, [], "   ", 8)).toEqual({ matches: [], create: null });
  });
});
