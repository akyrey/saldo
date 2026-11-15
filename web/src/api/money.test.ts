import { describe, expect, it } from "vitest";
import {
  formatMoney,
  fromMinor,
  isNegative,
  normalizeAmountInput,
  sumAmounts,
  toMinor,
} from "./money";

describe("toMinor", () => {
  it("parses whole and fractional amounts into minor units", () => {
    expect(toMinor("12.50", 2)).toBe(1250n);
    expect(toMinor("12.5", 2)).toBe(1250n);
    expect(toMinor("7", 2)).toBe(700n);
    expect(toMinor("-0.01", 2)).toBe(-1n);
    expect(toMinor("5", 0)).toBe(5n);
  });

  it("keeps precision far beyond float range", () => {
    expect(toMinor("90071992547409931.99", 2)).toBe(9007199254740993199n);
  });

  it("rejects malformed or too precise input", () => {
    for (const bad of ["", "1.", ".5", "1.234", "+1", "1e3", "1,00", " 1", "abc"]) {
      expect(toMinor(bad, 2), bad).toBeNull();
    }
  });
});

describe("fromMinor", () => {
  it("formats with exactly the given decimals", () => {
    expect(fromMinor(1250n, 2)).toBe("12.50");
    expect(fromMinor(5n, 2)).toBe("0.05");
    expect(fromMinor(-5n, 2)).toBe("-0.05");
    expect(fromMinor(0n, 2)).toBe("0.00");
    expect(fromMinor(42n, 0)).toBe("42");
    expect(fromMinor(1n, 8)).toBe("0.00000001");
  });

  it("round-trips with toMinor", () => {
    for (const text of ["0.00", "1.01", "-1234.56", "99999999999999.99"]) {
      expect(fromMinor(toMinor(text, 2) ?? 0n, 2)).toBe(text);
    }
  });
});

describe("normalizeAmountInput", () => {
  it("accepts a comma as decimal separator and trims spaces", () => {
    expect(normalizeAmountInput(" 12,5 ")).toBe("12.5");
    expect(normalizeAmountInput("1 000.25")).toBe("1000.25");
    expect(normalizeAmountInput("3.10")).toBe("3.10");
  });

  it("leaves ambiguous input for validation to reject", () => {
    expect(normalizeAmountInput("1,000.5")).toBe("1,000.5");
    expect(normalizeAmountInput("1,2,3")).toBe("1,2,3");
  });
});

describe("sumAmounts", () => {
  it("adds exactly where floats would not", () => {
    expect(sumAmounts(["0.10", "0.20"], 2)).toBe("0.30");
    expect(sumAmounts(["42.50", "17.25", "-0.75"], 2)).toBe("59.00");
  });

  it("skips invalid entries", () => {
    expect(sumAmounts(["1.00", "oops", ""], 2)).toBe("1.00");
  });
});

describe("formatting", () => {
  it("formats with the currency symbol and the string's own decimals", () => {
    expect(formatMoney("1234.50", "EUR", "en-US")).toBe("€1,234.50");
    expect(formatMoney("-3", "USD", "en-US")).toBe("-$3");
    expect(formatMoney("0.123456789", "EUR", "en-US")).toBe("€0.123456789");
  });

  it("adds a plus sign to positive amounts when asked", () => {
    expect(formatMoney("3.00", "EUR", "en-US", { signed: true })).toBe("+€3.00");
    expect(formatMoney("-3.00", "EUR", "en-US", { signed: true })).toBe("-€3.00");
    expect(formatMoney("0.00", "EUR", "en-US", { signed: true })).toBe("€0.00");
    expect(formatMoney("1.00", "NOT-A-CODE", "en-US", { signed: true })).toBe("+1.00 NOT-A-CODE");
  });

  it("falls back for unknown currency codes", () => {
    expect(formatMoney("1.00", "NOT-A-CODE", "en-US")).toBe("1.00 NOT-A-CODE");
  });

  it("detects negative amounts", () => {
    expect(isNegative("-1.00")).toBe(true);
    expect(isNegative("1.00")).toBe(false);
  });
});
