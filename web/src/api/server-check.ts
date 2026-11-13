// The desktop app's first screen asks for the server's address and checks
// it here, before anything else talks to it.

/** `money.example.com/` → `https://money.example.com`; `null` if unusable. */
export function normalizeServerUrl(input: string): string | null {
  const text = input.trim();
  if (text === "") return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `https://${text}`;
  try {
    const url = new URL(withScheme);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    if (url.hostname === "") return null;
    return `${url.origin}${url.pathname}`.replace(/\/+$/, "");
  } catch {
    return null;
  }
}

export type ServerCheck = { ok: true; version: string } | { ok: false; reason: string };

interface Health {
  status?: unknown;
  app?: unknown;
  api?: unknown;
  version?: unknown;
}

/** Asks `/healthz` whether `serverUrl` is a working Saldo server speaking API v1. */
export async function checkServer(serverUrl: string): Promise<ServerCheck> {
  let health: Health;
  try {
    const request = new Request(`${serverUrl}/healthz`, { credentials: "omit" });
    const response = await globalThis.fetch(request);
    health = (await response.json()) as Health;
  } catch {
    return { ok: false, reason: "The server cannot be reached." };
  }
  if (health.app !== "saldo" || typeof health.version !== "string") {
    return { ok: false, reason: "This address does not answer like a Saldo server." };
  }
  if (health.api !== "v1") {
    return { ok: false, reason: `This server speaks API ${String(health.api)}; the app needs v1.` };
  }
  if (health.status !== "ok") {
    return { ok: false, reason: "The server is running but its database is unavailable." };
  }
  return { ok: true, version: health.version };
}
