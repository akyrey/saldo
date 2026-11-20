import { describe, expect, it } from "vitest";
import { EMPTY_SEQUENCE, isPaletteKey, nextSequence, SEQUENCE_MS, SHORTCUTS } from "./shortcuts";

const key = (k: string, extra: Partial<KeyboardEventInit> = {}) =>
  new KeyboardEvent("keydown", { key: k, ...extra });

function press(keys: string[], gaps: number[] = []) {
  let state = EMPTY_SEQUENCE;
  let found: string | undefined;
  let now = 1000;
  keys.forEach((k, i) => {
    now += gaps[i] ?? 10;
    const result = nextSequence(state, k, now);
    state = result.state;
    if (result.shortcut) found = result.shortcut.id;
  });
  return found;
}

describe("nextSequence", () => {
  it("matches single keys and two-key sequences", () => {
    expect(press(["n"])).toBe("new-transaction");
    expect(press(["?"])).toBe("help");
    expect(press(["g", "t"])).toBe("go-transactions");
    expect(press(["g", "b"])).toBe("go-budgets");
  });

  it("forgets a sequence after the pause", () => {
    expect(press(["g", "t"], [0, SEQUENCE_MS + 1000])).toBeUndefined();
  });

  it("starts over on a key that continues nothing", () => {
    expect(press(["g", "x", "n"])).toBe("new-transaction");
    expect(press(["x"])).toBeUndefined();
  });
});

describe("isPaletteKey", () => {
  it("is Cmd+K or Ctrl+K only", () => {
    expect(isPaletteKey(key("k", { metaKey: true }))).toBe(true);
    expect(isPaletteKey(key("k", { ctrlKey: true }))).toBe(true);
    expect(isPaletteKey(key("k"))).toBe(false);
    expect(isPaletteKey(key("j", { metaKey: true }))).toBe(false);
  });
});

describe("SHORTCUTS", () => {
  it("has no key sequence twice", () => {
    const keys = SHORTCUTS.map((s) => s.keys.join(" "));
    expect(new Set(keys).size).toBe(keys.length);
  });
});
