import { browserPlatform } from "./browser";
import type { Platform } from "./types";

export type { AppUpdate, Platform, PlatformKind } from "./types";

let current: Platform = browserPlatform;

/** Where the app runs; the browser until `initPlatform` finds Tauri. */
export function platform(): Platform {
  return current;
}

/** Inside the desktop app, Tauri defines this before any page script runs. */
function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Picks the platform once, before the app renders. */
export async function initPlatform(): Promise<void> {
  if (!insideTauri()) return;
  const { createDesktopPlatform } = await import("./desktop");
  current = await createDesktopPlatform();
}

/** Tests replace the platform with a fake. */
export function setPlatformForTests(fake: Platform | null): void {
  current = fake ?? browserPlatform;
}
