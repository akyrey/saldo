// Pure helpers for the installed-app experience (Phase 7).

export interface InstallContext {
  userAgent: string;
  /** `navigator.maxTouchPoints`. */
  touchPoints: number;
  /** Already running as an installed app. */
  standalone: boolean;
  /** The browser fired `beforeinstallprompt` (Chrome, Edge, Android). */
  canPrompt: boolean;
}

/**
 * What to offer: the browser's own install prompt, instructions for Safari
 * on iPhone and iPad (which has no prompt), or nothing.
 */
export function installHint(context: InstallContext): "prompt" | "ios" | null {
  if (context.standalone) return null;
  if (context.canPrompt) return "prompt";
  const ua = context.userAgent;
  // iPadOS Safari reports itself as a Mac; a Mac has no touch screen.
  const appleMobile =
    /iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && context.touchPoints > 1);
  // Only Safari can add to the Home Screen with push support.
  const safari = /Safari/.test(ua) && !/CriOS|FxiOS|EdgiOS|OPiOS/.test(ua);
  return appleMobile && safari ? "ios" : null;
}

interface BillDue {
  active: boolean;
  next_due_date?: string | null;
}

/** Active bills whose earliest open period was due before `today` (YYYY-MM-DD). */
export function overdueCount(bills: readonly BillDue[], today: string): number {
  return bills.filter((b) => b.active && b.next_due_date != null && b.next_due_date < today).length;
}
