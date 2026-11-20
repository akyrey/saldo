// Keyboard shortcuts: one table that drives both the key handling
// (useShortcuts) and the "?" help overlay, so the two never disagree.

export type ShortcutAction =
  | { kind: "navigate"; to: string }
  | { kind: "new-transaction" }
  | { kind: "focus-search" }
  | { kind: "help" };

export interface Shortcut {
  id: string;
  /** Keys pressed one after the other, as `KeyboardEvent.key` values. */
  keys: readonly string[];
  label: string;
  group: "Go to" | "Actions" | "General";
  action: ShortcutAction;
}

const go = (key: string, label: string, to: string): Shortcut => ({
  id: `go-${label.toLowerCase()}`,
  keys: ["g", key],
  label,
  group: "Go to",
  action: { kind: "navigate", to },
});

export const SHORTCUTS: readonly Shortcut[] = [
  go("t", "Transactions", "/transactions"),
  go("a", "Accounts", "/accounts"),
  go("b", "Budgets", "/budgets"),
  go("r", "Reports", "/reports"),
  go("f", "Forecast", "/forecast"),
  {
    id: "new-transaction",
    keys: ["n"],
    label: "New transaction",
    group: "Actions",
    action: { kind: "new-transaction" },
  },
  {
    id: "focus-search",
    keys: ["/"],
    label: "Search transactions",
    group: "Actions",
    action: { kind: "focus-search" },
  },
  { id: "help", keys: ["?"], label: "Show shortcuts", group: "General", action: { kind: "help" } },
];

/** Keys handled outside the table, listed in the help overlay. */
export const EXTRA_KEYS: readonly { keys: string; label: string }[] = [
  { keys: "⌘K / Ctrl+K", label: "Open the command palette" },
  { keys: "⌘Enter / Ctrl+Enter", label: "Save, in an editor" },
  { keys: "Esc", label: "Close a dialog or menu" },
];

/** How long the second key of a sequence may wait for the first. */
export const SEQUENCE_MS = 1000;

export interface SequenceState {
  keys: readonly string[];
  at: number;
}

export const EMPTY_SEQUENCE: SequenceState = { keys: [], at: 0 };

const startsWith = (keys: readonly string[], prefix: readonly string[]) =>
  prefix.every((k, i) => keys[i] === k);

/**
 * Adds a key to the sequence. Returns the shortcut it completes, if any, and
 * the state to keep: the partial sequence while a shortcut can still follow,
 * otherwise empty. A key that continues nothing starts a new sequence.
 */
export function nextSequence(
  state: SequenceState,
  key: string,
  now: number,
): { state: SequenceState; shortcut?: Shortcut } {
  const fresh = now - state.at > SEQUENCE_MS;
  const attempt = (keys: readonly string[]) => {
    const exact = SHORTCUTS.find((s) => s.keys.length === keys.length && startsWith(s.keys, keys));
    if (exact) return { state: EMPTY_SEQUENCE, shortcut: exact };
    const partial = SHORTCUTS.some((s) => s.keys.length > keys.length && startsWith(s.keys, keys));
    return partial ? { state: { keys, at: now } } : null;
  };
  const continued = fresh || state.keys.length === 0 ? null : attempt([...state.keys, key]);
  return continued ?? attempt([key]) ?? { state: EMPTY_SEQUENCE };
}

/** Cmd+K on macOS, Ctrl+K elsewhere (both accepted everywhere). */
export function isPaletteKey(event: KeyboardEvent): boolean {
  return (event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === "k";
}

/** Typing in a field must never trigger a shortcut. */
export function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}
