// "Log in with Authentik" in the desktop app (RFC 8252): the login runs in
// the system browser, which comes back to the app on a local port with a
// one-time code. The app trades code plus its secret verifier for the token.
import { api, call, desktopOidcStartUrl } from "@/api/client";
import { platform } from "@/platform";

/** Bytes of randomness in the verifier (it becomes 43 base64url characters). */
const VERIFIER_BYTES = 32;

/** Why the provider login did not complete; `code` is a login page error code. */
export class ProviderLoginError extends Error {
  constructor(readonly code: string) {
    super(code);
    this.name = "ProviderLoginError";
  }
}

function base64url(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

/** A random verifier and its S256 challenge (what the server sees first). */
async function verifierPair(): Promise<{ verifier: string; challenge: string }> {
  const verifier = base64url(crypto.getRandomValues(new Uint8Array(VERIFIER_BYTES)));
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  return { verifier, challenge: base64url(new Uint8Array(digest)) };
}

/** Logs in through the browser and keeps the token; throws `ProviderLoginError`. */
export async function loginWithProviderOnDesktop(): Promise<void> {
  const current = platform();
  const { verifier, challenge } = await verifierPair();
  const { port, result } = await current.waitForLoginCallback();
  await current.openExternal(desktopOidcStartUrl(current.serverUrl(), port, challenge));
  const answer = await result;
  if (!answer.code) throw new ProviderLoginError(answer.error ?? "oidc_refused");
  const login = await call(
    api.POST("/api/v1/auth/oidc/desktop-token", { body: { code: answer.code, verifier } }),
  );
  if (!login.data.token) throw new ProviderLoginError("oidc_unavailable");
  await current.saveToken(login.data.token);
}
