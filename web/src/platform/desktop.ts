// The desktop app: the only file (with index.ts) that talks to Tauri.
// `invoke` calls a `#[tauri::command]` in desktop/src-tauri/src/lib.rs, the
// way a Go client calls an RPC handler. Loaded only inside Tauri, so the
// browser bundle never contains it.
import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Platform } from "./types";

const SERVER_KEY = "saldo.serverUrl";

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
  };
}
