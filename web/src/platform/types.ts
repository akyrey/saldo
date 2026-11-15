// What differs between the web app in a browser and the desktop app (Tauri).
// The rest of the SPA asks `platform()` instead of checking where it runs,
// like a Go interface with two implementations picked once at start-up.

export type PlatformKind = "browser" | "desktop";

/** What the browser brought back to the desktop app after a provider login. */
export interface LoginCallback {
  code?: string;
  error?: string;
}

/** A newer desktop version, found by `Platform.checkForUpdate`. */
export interface AppUpdate {
  readonly version: string;
  readonly currentVersion: string;
  /**
   * Downloads and installs the update, then restarts the app. `onProgress`
   * gets the share downloaded (0 to 1), or `null` while the size is unknown.
   */
  install(onProgress: (fraction: number | null) => void): Promise<void>;
}

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
  /**
   * Desktop only: listens on a local port for the browser's return from
   * "Log in with Authentik". `result` settles when it arrives.
   */
  waitForLoginCallback(): Promise<{ port: number; result: Promise<LoginCallback> }>;
  /** Desktop: asks the release feed for a newer version. Browser: always `null`. */
  checkForUpdate(): Promise<AppUpdate | null>;
}
