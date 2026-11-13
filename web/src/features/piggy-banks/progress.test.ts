import { describe, expect, it } from "vitest";
import { progressPercent, signedAmount } from "./progress";

describe("progressPercent", () => {
  it("rounds down to whole percent with exact maths", () => {
    expect(progressPercent("0.00", "300.00", 2)).toBe(0);
    expect(progressPercent("100.00", "300.00", 2)).toBe(33);
    expect(progressPercent("299.99", "300.00", 2)).toBe(99);
    expect(progressPercent("300.00", "300.00", 2)).toBe(100);
  });

  it("caps at 100 and floors at 0", () => {
    expect(progressPercent("400.00", "300.00", 2)).toBe(100);
    expect(progressPercent("-5.00", "300.00", 2)).toBe(0);
  });

  it("is null without a usable target", () => {
    expect(progressPercent("10.00", null, 2)).toBeNull();
    expect(progressPercent("10.00", "0.00", 2)).toBeNull();
    expect(progressPercent("x", "10.00", 2)).toBeNull();
  });
});

describe("signedAmount", () => {
  it("normalises the input and negates removals", () => {
    expect(signedAmount("add", " 12,5 ")).toBe("12.5");
    expect(signedAmount("remove", "10.00")).toBe("-10.00");
  });

  it("leaves an explicit sign for the server to judge", () => {
    expect(signedAmount("remove", "-3")).toBe("-3");
    expect(signedAmount("add", "")).toBe("");
  });
});
