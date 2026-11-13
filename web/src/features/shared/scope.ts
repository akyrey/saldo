/**
 * The scope switch: whose accounts lists show. Stored in the URL as
 * `?scope=mine` or `?scope=user:<id>`; missing means the whole household.
 */
export type Scope = "household" | "mine" | `user:${string}`;

export function parseScope(value: unknown): Scope | undefined {
  if (value === "mine" || value === "household") return value;
  if (typeof value === "string" && /^user:[0-9a-f-]{36}$/i.test(value)) {
    return value as Scope;
  }
  return undefined;
}

/** The query value for the API; `undefined` lets the server default to household. */
export function scopeParam(scope: Scope | undefined): string | undefined {
  return scope === "household" ? undefined : scope;
}
