import { describe, expect, it } from "vitest";
import { parseScope, scopeParam } from "./scope";

describe("scope", () => {
  it("accepts the three forms", () => {
    expect(parseScope("mine")).toBe("mine");
    expect(parseScope("household")).toBe("household");
    const id = "0190f5c3-7a2b-7c00-8000-000000000001";
    expect(parseScope(`user:${id}`)).toBe(`user:${id}`);
  });

  it("drops anything else", () => {
    for (const bad of [undefined, 3, "", "all", "user:", "user:nope"]) {
      expect(parseScope(bad)).toBeUndefined();
    }
  });

  it("omits household from the API query", () => {
    expect(scopeParam("household")).toBeUndefined();
    expect(scopeParam(undefined)).toBeUndefined();
    expect(scopeParam("mine")).toBe("mine");
  });
});
