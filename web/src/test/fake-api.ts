import { vi } from "vitest";

/** A canned answer: a body for 200, or a full Response. */
type Answer = unknown;
type Handler = (request: Request) => Answer | Promise<Answer>;

export function json(status: number, body: unknown): Response {
  return new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

export function apiError(status: number, code: string, message = code): Response {
  return json(status, { error: { code, message, details: [] } });
}

/**
 * Replaces `fetch` with a router of `"METHOD /path"` → handler. Unknown
 * routes answer 404. Returns the requests seen, for assertions.
 */
export function fakeApi(routes: Record<string, Handler | Answer>): Request[] {
  const seen: Request[] = [];
  vi.stubGlobal("fetch", async (request: Request) => {
    seen.push(request.clone());
    const key = `${request.method} ${new URL(request.url).pathname}`;
    if (!(key in routes)) return apiError(404, "not_found", key);
    const route = routes[key];
    const answer = typeof route === "function" ? await (route as Handler)(request) : route;
    return answer instanceof Response ? answer : json(200, answer);
  });
  return seen;
}

const meta = (total: number) => ({ page: 1, per_page: 200, total });
export const list = <T>(items: T[]) => ({ data: items, meta: meta(items.length) });

export const HOUSEHOLD = {
  id: "h1",
  name: "Home",
  primary_currency: "EUR",
  timezone: "UTC",
  created_at: "",
  updated_at: "",
};
export const ME = {
  data: {
    user: { id: "u1", email: "ada@example.com", display_name: "Ada", created_at: "" },
    household: HOUSEHOLD,
    role: "owner",
  },
};
export const MEMBERS = {
  data: [
    { user_id: "u1", email: "ada@example.com", display_name: "Ada", role: "owner", joined_at: "" },
    {
      user_id: "0190f5c3-7a2b-7c00-8000-000000000002",
      email: "bo@example.com",
      display_name: "Bo",
      role: "member",
      joined_at: "",
    },
  ],
};
export const CURRENCIES = { data: [{ code: "EUR", name: "Euro", symbol: "€", decimal_places: 2 }] };

export function account(id: string, name: string, type: string, extra: object = {}) {
  return {
    id,
    name,
    type,
    currency: "EUR",
    virtual_balance: "0.00",
    include_net_worth: true,
    active: true,
    sort_order: 0,
    created_at: "",
    updated_at: "",
    owner_user_id: null,
    ...extra,
  };
}

interface FakeBalance {
  account_id: string;
  balance: string;
  currency: string;
}

/**
 * `GET /accounts` and `GET /accounts/balances` that honour the `type`,
 * `active`, `q`, `page` and `per_page` filters like the server does.
 */
export function accountRoutes(
  accounts: ReturnType<typeof account>[],
  balances: FakeBalance[],
): Record<string, Handler> {
  const matching = (request: Request) => {
    const params = new URL(request.url).searchParams;
    const type = params.get("type");
    const active = params.get("active");
    const q = params.get("q")?.trim().toLowerCase();
    const found = accounts.filter(
      (a) =>
        (!type || a.type === type) &&
        (active === null || String(a.active) === active) &&
        (!q || a.name.toLowerCase().includes(q)),
    );
    const page = Number(params.get("page") ?? "1");
    const perPage = Number(params.get("per_page") ?? "50");
    const items = found.slice((page - 1) * perPage, page * perPage);
    return { items, meta: { page, per_page: perPage, total: found.length } };
  };
  return {
    "GET /api/v1/accounts": (request) => {
      const { items, meta } = matching(request);
      return { data: items, meta };
    },
    "GET /api/v1/accounts/balances": (request) => {
      const { items, meta } = matching(request);
      const data = items.flatMap((a) => balances.filter((b) => b.account_id === a.id));
      return { data, meta };
    },
  };
}

export function split(
  id: string,
  source: string,
  destination: string,
  amount: string,
  kind = "withdrawal",
) {
  return {
    id,
    position: 0,
    kind,
    date: "2026-11-07",
    description: `split ${id}`,
    source_account_id: source,
    destination_account_id: destination,
    amount,
    currency: "EUR",
    tag_ids: [],
    reconciled: false,
    meta: {},
    created_at: "",
    updated_at: "",
  };
}
