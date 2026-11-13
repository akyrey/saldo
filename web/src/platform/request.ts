/** Where API requests go and how they authenticate. */
export interface ServerTarget {
  /** `""`: the page's own origin, as in the browser. */
  serverUrl: string;
  token: string | null;
}

/**
 * The browser's request unchanged, or, for the desktop app, the same request
 * sent to the configured server with `Authorization: Bearer` and no cookies.
 * The CSRF header is dropped: bearer requests skip that check, and every
 * extra header would need its own CORS allowance.
 */
export async function toServerRequest(request: Request, target: ServerTarget): Promise<Request> {
  if (target.serverUrl === "") return request;
  const url = new URL(request.url);
  const headers = new Headers(request.headers);
  headers.delete("X-Saldo-Csrf");
  if (target.token) headers.set("Authorization", `Bearer ${target.token}`);
  const hasBody = request.method !== "GET" && request.method !== "HEAD";
  return new Request(`${target.serverUrl}${url.pathname}${url.search}`, {
    method: request.method,
    headers,
    body: hasBody ? await request.arrayBuffer() : undefined,
    credentials: "omit",
    signal: request.signal,
  });
}
