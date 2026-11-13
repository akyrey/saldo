import { describe, expect, it } from "vitest";
import { safeRedirect } from "./safe-redirect";

describe("safeRedirect", () => {
  it("allows local paths", () => {
    expect(safeRedirect("/transactions?page=2")).toBe("/transactions?page=2");
  });

  it("rejects other sites and non-strings", () => {
    for (const bad of ["//evil.example", "https://evil.example", "/\\evil", 42, undefined]) {
      expect(safeRedirect(bad)).toBeUndefined();
    }
  });
});
