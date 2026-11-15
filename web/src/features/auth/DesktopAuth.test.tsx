// The desktop app's first run: choose the server, log in with a bearer token,
// log out again. The platform is a fake desktop; the API is faked as usual.
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setPlatformForTests, type Platform } from "@/platform";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
  setPlatformForTests(null);
});

function fakeDesktop(serverUrl = "", token: string | null = null) {
  const state = { serverUrl, token };
  const fake: Platform = {
    kind: "desktop",
    serverUrl: () => state.serverUrl,
    token: () => state.token,
    setServerUrl: (url) => {
      state.serverUrl = url;
    },
    saveToken: vi.fn(async (value: string) => {
      state.token = value;
    }),
    clearToken: vi.fn(async () => {
      state.token = null;
    }),
    notify: vi.fn(async () => {}),
    openExternal: vi.fn(async () => {}),
    waitForLoginCallback: async () => ({
      port: 53682,
      result: Promise.resolve({ code: "one-time" }),
    }),
  };
  setPlatformForTests(fake);
  return { fake, state };
}

const HEALTH = { status: "ok", app: "saldo", api: "v1", version: "0.1.0" };

function api(loggedIn: () => boolean) {
  return fakeApi({
    "GET /healthz": HEALTH,
    "GET /api/v1/setup": { data: { needed: false } },
    "GET /api/v1/auth/me": () =>
      loggedIn()
        ? Response.json(ME)
        : Response.json({ error: { code: "unauthorized", message: "no" } }, { status: 401 }),
    "POST /api/v1/auth/login": {
      data: { ...ME.data, household_id: "h1", expires_at: "", token: "desktop-token" },
    },
    "POST /api/v1/auth/logout": () => new Response(null, { status: 204 }),
    "GET /api/v1/auth/methods": { data: { password: true, oidc: { name: "Authentik" } } },
    "POST /api/v1/auth/oidc/desktop-token": {
      data: { ...ME.data, household_id: "h1", expires_at: "", token: "provider-token" },
    },
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/accounts/balances": list([]),
  });
}

describe("desktop first run", () => {
  it("asks for the server, then logs in with a bearer token", async () => {
    const { fake, state } = fakeDesktop();
    const seen = api(() => state.token !== null);
    const { router } = renderApp("/accounts");

    await waitFor(() => expect(router.state.location.pathname).toBe("/connect"));
    await userEvent.type(await screen.findByLabelText("Server address"), "money.example.com/");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/login"));
    expect(state.serverUrl).toBe("https://money.example.com");

    await userEvent.type(await screen.findByLabelText("Email"), "ada@example.com");
    await userEvent.type(screen.getByLabelText("Password"), "a long password");
    await userEvent.click(screen.getByRole("button", { name: "Log in" }));

    await waitFor(() => expect(router.state.location.pathname).toBe("/transactions"));
    expect(fake.saveToken).toHaveBeenCalledWith("desktop-token");
    const login = seen.find((r) => r.url.endsWith("/api/v1/auth/login"));
    expect(login?.url).toBe("https://money.example.com/api/v1/auth/login");
    expect(await login?.json()).toMatchObject({ client: "desktop" });
    const me = seen.filter((r) => r.url.endsWith("/api/v1/auth/me")).pop();
    expect(me?.headers.get("Authorization")).toBe("Bearer desktop-token");
  });

  it("explains an address that is not a Saldo server", async () => {
    fakeDesktop();
    fakeApi({ "GET /healthz": { hello: "world" } });
    const { router } = renderApp("/connect");
    await userEvent.type(await screen.findByLabelText("Server address"), "https://example.com");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(await screen.findByText(/does not answer like a Saldo server/)).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/connect");
  });

  it("logs in with the identity provider through the system browser", async () => {
    const { fake, state } = fakeDesktop("https://money.example.com");
    api(() => state.token !== null);
    const { router } = renderApp("/login");

    await userEvent.click(await screen.findByRole("button", { name: "Log in with Authentik" }));

    await waitFor(() => expect(router.state.location.pathname).toBe("/transactions"));
    expect(fake.openExternal).toHaveBeenCalledWith(
      expect.stringContaining(
        "https://money.example.com/api/v1/auth/oidc/start?desktop_port=53682",
      ),
    );
    expect(fake.saveToken).toHaveBeenCalledWith("provider-token");
  });

  it("forgets the token on logout", async () => {
    const { fake, state } = fakeDesktop("https://money.example.com", "desktop-token");
    api(() => state.token !== null);
    const { router } = renderApp("/accounts");
    await userEvent.click(await screen.findByRole("button", { name: "Log out" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/login"));
    expect(fake.clearToken).toHaveBeenCalled();
  });
});
