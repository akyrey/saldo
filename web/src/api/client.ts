// The only place that knows the API's URLs. Types come from `schema.d.ts`,
// which `npm run gen:api` generates from `web/openapi.json` — like generating
// a Go client from a proto or OpenAPI file: when the server contract changes,
// regenerating makes the compiler point at every call that no longer fits.
import createClient, { type Middleware } from "openapi-fetch";
import { platform } from "@/platform";
import { toServerRequest } from "@/platform/request";
import { ApiError, apiErrorFrom } from "./errors";
import type { paths } from "./schema";

/** Called when the server answers 401 (session expired or logged out). */
type UnauthorizedListener = () => void;

let onUnauthorized: UnauthorizedListener = () => {};

export function setUnauthorizedListener(listener: UnauthorizedListener): void {
  onUnauthorized = listener;
}

// Cookie-authenticated writes need this header (the server's CSRF check).
// Sending it on every request is harmless and keeps the rule simple.
const csrf: Middleware = {
  onRequest({ request }) {
    request.headers.set("X-Saldo-Csrf", "1");
    return request;
  },
  onResponse({ response, schemaPath }) {
    if (response.status === 401 && schemaPath !== "/api/v1/auth/login") onUnauthorized();
    return response;
  },
};

/**
 * Sends one request: unchanged in the browser; to the configured server with
 * the bearer token in the desktop app. Every API call, download and the
 * desktop event stream go through here.
 */
export async function sendRequest(request: Request): Promise<Response> {
  const current = platform();
  const target = { serverUrl: current.serverUrl(), token: current.token() };
  // Look `fetch` up on every call (not once at start), so tests can replace it.
  return globalThis.fetch(await toServerRequest(request, target));
}

/**
 * Creates a client. `baseUrl` is empty for the web app and the desktop app
 * (the platform supplies the server); a development build may set it.
 */
export function createApi(baseUrl: string) {
  const client = createClient<paths>({
    baseUrl,
    credentials: "include",
    fetch: sendRequest,
  });
  client.use(csrf);
  return client;
}

export type Api = ReturnType<typeof createApi>;

/**
 * Where "Log in with Authentik" starts: a full-page navigation (the provider
 * shows its own login page), so a URL instead of an API call.
 */
/**
 * The desktop app's "Log in with Authentik": opened in the system browser,
 * which comes back to the app on `port` (see desktop/src-tauri/src/loopback.rs).
 */
export function desktopOidcStartUrl(serverUrl: string, port: number, challenge: string): string {
  const query = new URLSearchParams({
    desktop_port: String(port),
    desktop_challenge: challenge,
  });
  return `${serverUrl}/api/v1/auth/oidc/start?${query.toString()}`;
}

export function oidcStartUrl(redirect?: string): string {
  const start = `${import.meta.env.VITE_SALDO_API_URL ?? ""}/api/v1/auth/oidc/start`;
  return redirect ? `${start}?redirect=${encodeURIComponent(redirect)}` : start;
}

export const api: Api = createApi(import.meta.env.VITE_SALDO_API_URL ?? "");

/** What every openapi-fetch call resolves to, reduced to what we need. */
interface FetchResult<T> {
  data?: T;
  error?: unknown;
  response: Response;
}

/**
 * Returns the body of a successful call or throws an `ApiError`. A failed
 * network request (offline) becomes an `ApiError` with code `network`.
 */
export async function call<T>(request: Promise<FetchResult<T>>): Promise<T> {
  let result: FetchResult<T>;
  try {
    result = await request;
  } catch (cause) {
    throw new ApiError(0, "network", cause instanceof Error ? cause.message : "network error");
  }
  if (result.error !== undefined || !result.response.ok) {
    throw apiErrorFrom(result.response.status, result.error);
  }
  return result.data as T;
}
