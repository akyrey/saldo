// What differs between the web app in a browser and the desktop app (Tauri).
// The rest of the SPA asks `platform()` instead of checking where it runs,
// like a Go interface with two implementations picked once at start-up.

export type PlatformKind = "browser" | "desktop";

export interface Platform {
  readonly kind: PlatformKind;
  /** The server's address; `""` means the page's own origin (the browser). */
  serverUrl(): string;
  /** The bearer token, or `null` when the browser's session cookie is used. */
  token(): string | null;
  /** Desktop: remembers the server chosen at onboarding. Browser: ignored. */
  setServerUrl(url: string): void;
  /** Keeps the token from a desktop login (in the OS keychain). */
  saveToken(token: string): Promise<void>;
  /** Forgets the token at logout. */
  clearToken(): Promise<void>;
  /** Shows a system notification (desktop) or nothing (the browser uses Web Push). */
  notify(title: string, body: string): Promise<void>;
  /** Opens a link in the person's default browser. */
  openExternal(url: string): Promise<void>;
}
