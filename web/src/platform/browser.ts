import type { Platform } from "./types";

/** The web app: same origin, cookie session, the browser's own windows. */
export const browserPlatform: Platform = {
  kind: "browser",
  serverUrl: () => "",
  token: () => null,
  setServerUrl: () => {},
  saveToken: async () => {},
  clearToken: async () => {},
  notify: async () => {},
  openExternal: async (url) => {
    window.open(url, "_blank", "noopener,noreferrer");
  },
  // The browser follows the provider's redirects itself.
  waitForLoginCallback: () =>
    Promise.reject(new Error("only the desktop app waits for a login callback")),
  // The server updates the web app; a reload picks it up.
  checkForUpdate: async () => null,
  setTheme: async () => {},
};
