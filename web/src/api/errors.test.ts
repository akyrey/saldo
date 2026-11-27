import { describe, expect, it } from "vitest";
import { ApiError, explain } from "./errors";

describe("explain", () => {
  it("prefers the field messages over the general one", () => {
    const error = new ApiError(422, "validation_failed", "the request has an invalid field", [
      { code: "invalid", field: "file", message: 'no header row with a column named ["Data"]' },
      { code: "invalid", field: "lines", message: "row 2 mixes money in and out" },
    ]);
    expect(explain(error)).toBe(
      'no header row with a column named ["Data"]; row 2 mixes money in and out',
    );
  });

  it("falls back to the general message and to plain errors", () => {
    expect(explain(new ApiError(409, "conflict", "the import is committed, not a draft"))).toBe(
      "the import is committed, not a draft",
    );
    expect(explain(new Error("The mapping is not valid JSON."))).toBe(
      "The mapping is not valid JSON.",
    );
    expect(explain("nope")).toBe("Something went wrong.");
  });
});
