// The desktop app: the only file (with index.ts) that talks to Tauri.
// `invoke` calls a `#[tauri::command]` in desktop/src-tauri/src/lib.rs, the
// way a Go client calls an RPC handler. Loaded only inside Tauri, so the
// browser bundle never contains it.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import type { AppUpdate, LoginCallback, Platform } from "./types";
import { trackDownload } from "./update-progress";

const SERVER_KEY = "saldo.serverUrl";

/** Listen first, then open the port, so the answer cannot be missed. */
async function waitForLoginCallback() {
  let settle: (answer: LoginCallback) => void = () => {};
  const result = new Promise<LoginCallback>((resolve) => {
    settle = resolve;
  });
  const stop = await listen<LoginCallback>("oidc-callback", (event) => {
    stop();
    settle(event.payload);
  });
  const port = await invoke<number>("oidc_listen");
  return { port, result };
}

function readServerUrl(): string {
  try {
    return localStorage.getItem(SERVER_KEY) ?? "";
  } catch {
    return "";
  }
}

async function canNotify(): Promise<boolean> {
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

/**
 * Reads `latest.json` from the newest GitHub release (tauri.conf.json,
 * plugins.updater). The Rust side checks the download's signature against
 * the public key in the config before installing it.
 */
async function checkForUpdate(): Promise<AppUpdate | null> {
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    currentVersion: update.currentVersion,
    install: async (onProgress) => {
      await update.downloadAndInstall(trackDownload(onProgress));
      // Windows: the installer has already closed the app by now.
      await relaunch();
    },
  };
}

export async function createDesktopPlatform(): Promise<Platform> {
  let serverUrl = readServerUrl();
  let token: string | null = await invoke<string | null>("load_token");
  return {
    kind: "desktop",
    serverUrl: () => serverUrl,
    token: () => token,
    setServerUrl: (url) => {
      serverUrl = url;
      try {
        localStorage.setItem(SERVER_KEY, url);
      } catch {
        // The address is asked again next start; nothing else depends on it.
      }
    },
    saveToken: async (value) => {
      await invoke("save_token", { token: value });
      token = value;
    },
    clearToken: async () => {
      token = null;
      await invoke("delete_token");
    },
    notify: async (title, body) => {
      if (await canNotify()) sendNotification({ title, body });
    },
    openExternal: (url) => openUrl(url),
    waitForLoginCallback,
    checkForUpdate,
  };
}
