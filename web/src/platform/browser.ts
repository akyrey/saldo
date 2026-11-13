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
};
