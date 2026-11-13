// "Log in with Authentik" on the login page.
import { screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeApi } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

function api(methods: unknown) {
  return fakeApi({
    "GET /api/v1/auth/methods": { data: methods },
    "GET /api/v1/setup": { data: { needed: false } },
  });
}

describe("login with the identity provider", () => {
  it("offers the provider next to the password form, keeping where to go back to", async () => {
    api({ password: true, oidc: { name: "Authentik" } });
    renderApp("/login?redirect=%2Fbudgets");
    const link = await screen.findByRole("link", { name: "Log in with Authentik" });
    const href = new URL(link.getAttribute("href") ?? "", "http://x");
    expect(href.pathname).toBe("/api/v1/auth/oidc/start");
    expect(href.searchParams.get("redirect")).toBe("/budgets");
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
  });

  it("shows only the provider when passwords are turned off", async () => {
    api({ password: false, oidc: { name: "Authentik" } });
    renderApp("/login");
    expect(await screen.findByRole("link", { name: "Log in with Authentik" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Password")).toBeNull();
  });

  it("explains why a provider login failed", async () => {
    api({ password: true, oidc: { name: "Authentik" } });
    renderApp("/login?error=oidc_no_account");
    expect(await screen.findByText(/no Saldo member has this email/i)).toBeInTheDocument();
  });

  it("shows no provider button when none is configured", async () => {
    api({ password: true, oidc: null });
    renderApp("/login");
    expect(await screen.findByLabelText("Password")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Log in with/ })).toBeNull();
  });
});
