import { afterEach, describe, expect, it, vi } from "vitest";
import { setPlatformForTests, type Platform } from "@/platform";
import { browserPlatform } from "@/platform/browser";
import { fakeApi, ME } from "@/test/fake-api";
import { loginWithProviderOnDesktop, ProviderLoginError } from "./desktop-login";

afterEach(() => {
  vi.unstubAllGlobals();
  setPlatformForTests(null);
});

function desktop(answer: { code?: string; error?: string }) {
  const opened: string[] = [];
  const fake: Platform = {
    ...browserPlatform,
    kind: "desktop",
    serverUrl: () => "https://money.example.com",
    saveToken: vi.fn(async () => {}),
    openExternal: async (url) => {
      opened.push(url);
    },
    waitForLoginCallback: async () => ({ port: 53682, result: Promise.resolve(answer) }),
  };
  setPlatformForTests(fake);
  return { fake, opened };
}

describe("loginWithProviderOnDesktop", () => {
  it("logs in through the browser and keeps the token", async () => {
    const seen = fakeApi({
      "POST /api/v1/auth/oidc/desktop-token": {
        data: { ...ME.data, household_id: "h1", expires_at: "", token: "desktop-token" },
      },
    });
    const { fake, opened } = desktop({ code: "one-time" });

    await loginWithProviderOnDesktop();

    const url = new URL(opened[0] ?? "");
    expect(url.origin + url.pathname).toBe("https://money.example.com/api/v1/auth/oidc/start");
    expect(url.searchParams.get("desktop_port")).toBe("53682");
    const challenge = url.searchParams.get("desktop_challenge") ?? "";
    expect(challenge).toMatch(/^[A-Za-z0-9_-]{43}$/);
    const body = (await seen[0]?.json()) as { code: string; verifier: string };
    expect(body.code).toBe("one-time");
    // The challenge is the S256 hash of the verifier the app keeps to itself.
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(body.verifier));
    const expected = btoa(String.fromCharCode(...new Uint8Array(digest)))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/, "");
    expect(challenge).toBe(expected);
    expect(fake.saveToken).toHaveBeenCalledWith("desktop-token");
  });

  it("reports why the provider login failed", async () => {
    fakeApi({});
    desktop({ error: "oidc_no_account" });
    await expect(loginWithProviderOnDesktop()).rejects.toEqual(
      new ProviderLoginError("oidc_no_account"),
    );
  });
});
