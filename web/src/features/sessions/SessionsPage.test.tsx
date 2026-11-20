// The login sessions page inside the whole app, against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const HERE = {
  id: "s1",
  client: "browser",
  created_at: "2026-11-20T09:00:00.000Z",
  last_used_at: "2026-11-20T10:00:00.000Z",
  expires_at: "2026-12-20T09:00:00.000Z",
  current: true,
};
const DESKTOP = {
  id: "s2",
  client: "desktop",
  created_at: "2026-11-18T09:00:00.000Z",
  last_used_at: null,
  expires_at: "2026-12-18T09:00:00.000Z",
  current: false,
};
const OLD = { ...DESKTOP, id: "s3", client: null };

/** The session rows, in the order shown. */
async function sessionRows(): Promise<HTMLElement[]> {
  return screen.findAllByTestId("session");
}

function nth(rows: HTMLElement[], index: number): HTMLElement {
  const row = rows[index];
  if (!row) throw new Error(`no session row ${index}`);
  return row;
}

function api(sessions: unknown[], routes: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/auth/sessions": { data: sessions },
    ...routes,
  });
}

describe("sessions page", () => {
  it("lists the logins and marks this device", async () => {
    api([HERE, DESKTOP, OLD]);
    renderApp("/settings/sessions");
    const rows = await sessionRows();
    expect(rows).toHaveLength(3);
    expect(within(nth(rows, 0)).getByText("Web browser")).toBeInTheDocument();
    expect(within(nth(rows, 0)).getByText("This device")).toBeInTheDocument();
    expect(within(nth(rows, 0)).queryByRole("button", { name: "Sign out" })).toBeNull();
    expect(within(nth(rows, 1)).getByText("Desktop app")).toBeInTheDocument();
    expect(within(nth(rows, 1)).getByText(/Never used/)).toBeInTheDocument();
    expect(within(nth(rows, 2)).getByText("Unknown app")).toBeInTheDocument();
  });

  it("signs one other device out", async () => {
    const seen = api([HERE, DESKTOP], {
      "DELETE /api/v1/auth/sessions/s2": new Response(null, { status: 204 }),
    });
    renderApp("/settings/sessions");
    const rows = await sessionRows();
    await userEvent.click(within(nth(rows, 1)).getByRole("button", { name: "Sign out" }));
    await waitFor(() =>
      expect(seen.some((r) => r.method === "DELETE" && r.url.endsWith("/sessions/s2"))).toBe(true),
    );
  });

  it("signs out everywhere else and says how many", async () => {
    const seen = api([HERE, DESKTOP, OLD], {
      "POST /api/v1/auth/sessions/revoke-others": { data: { revoked: 2 } },
    });
    renderApp("/settings/sessions");
    await userEvent.click(await screen.findByRole("button", { name: "Sign out everywhere else" }));
    expect(await screen.findByText("Signed out 2 other sessions.")).toBeInTheDocument();
    expect(seen.some((r) => r.url.endsWith("/sessions/revoke-others"))).toBe(true);
  });

  it("offers no bulk sign-out when this is the only session", async () => {
    api([HERE]);
    renderApp("/settings/sessions");
    await screen.findAllByTestId("session");
    expect(screen.queryByRole("button", { name: "Sign out everywhere else" })).toBeNull();
  });
});
