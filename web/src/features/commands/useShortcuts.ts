import { useEffect, useRef } from "react";
import {
  EMPTY_SEQUENCE,
  isEditable,
  isPaletteKey,
  nextSequence,
  type SequenceState,
  type ShortcutAction,
} from "./shortcuts";

interface ShortcutHandlers {
  onAction: (action: ShortcutAction) => void;
  onPalette: () => void;
}

/** A dialog (the palette, the help, a confirmation) owns the keyboard while open. */
function dialogOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"]') !== null;
}

/** ⌘Enter / Ctrl+Enter inside a form saves it, like its submit button. */
function submitsForm(event: KeyboardEvent): boolean {
  if (event.key !== "Enter" || !(event.metaKey || event.ctrlKey)) return false;
  const form = event.target instanceof HTMLElement ? event.target.closest("form") : null;
  if (!form) return false;
  event.preventDefault();
  form.requestSubmit();
  return true;
}

/** One `keydown` listener on the window for every shortcut in the table. */
export function useShortcuts({ onAction, onPalette }: ShortcutHandlers): void {
  // The latest handlers, read inside the listener without re-subscribing.
  const handlers = useRef({ onAction, onPalette });
  useEffect(() => {
    handlers.current = { onAction, onPalette };
  });

  useEffect(() => {
    let sequence: SequenceState = EMPTY_SEQUENCE;
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      if (isPaletteKey(event)) {
        event.preventDefault();
        handlers.current.onPalette();
        return;
      }
      if (submitsForm(event)) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (isEditable(event.target) || dialogOpen()) return;
      const result = nextSequence(sequence, event.key, event.timeStamp || Date.now());
      sequence = result.state;
      if (result.shortcut) {
        event.preventDefault();
        handlers.current.onAction(result.shortcut.action);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
