import { afterEach, describe, expect, it, vi } from "vitest";
import { applyTheme, parseChoice, readChoice, resolveTheme, THEME_KEY, writeChoice } from "./theme";

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  document.documentElement.classList.remove("dark");
});

describe("resolveTheme", () => {
  it("follows the system only when the choice is system", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });
});

describe("parseChoice", () => {
  it("accepts the three choices and treats anything else as system", () => {
    expect(parseChoice("dark")).toBe("dark");
    expect(parseChoice("light")).toBe("light");
    expect(parseChoice("system")).toBe("system");
    expect(parseChoice("purple")).toBe("system");
    expect(parseChoice(null)).toBe("system");
  });
});

describe("readChoice and writeChoice", () => {
  it("round-trips through localStorage", () => {
    writeChoice("dark");
    expect(localStorage.getItem(THEME_KEY)).toBe("dark");
    expect(readChoice()).toBe("dark");
  });

  it("falls back to system when storage throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readChoice()).toBe("system");
  });

  it("ignores a storage that refuses writes", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(() => writeChoice("light")).not.toThrow();
  });
});

describe("applyTheme", () => {
  it("toggles the dark class, color-scheme and the theme-color meta", () => {
    const meta = document.createElement("meta");
    meta.name = "theme-color";
    document.head.append(meta);
    applyTheme("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(meta.content).toBe("#0a0a0a");
    applyTheme("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(meta.content).toBe("#0f766e");
    meta.remove();
  });
});
