import { afterEach, describe, expect, it, vi } from "vitest";
import { checkServer, normalizeServerUrl } from "./server-check";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("normalizeServerUrl", () => {
  it("keeps the origin and an optional path, without a trailing slash", () => {
    expect(normalizeServerUrl(" https://money.example.com/ ")).toBe("https://money.example.com");
    expect(normalizeServerUrl("http://192.168.1.5:8080")).toBe("http://192.168.1.5:8080");
    expect(normalizeServerUrl("money.example.com")).toBe("https://money.example.com");
  });

  it("refuses what is not an http(s) address", () => {
    expect(normalizeServerUrl("ftp://x")).toBeNull();
    expect(normalizeServerUrl("")).toBeNull();
    expect(normalizeServerUrl("https://")).toBeNull();
  });
});

function health(body: unknown, status = 200) {
  const fetch = vi.fn(async () => Response.json(body, { status }));
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

describe("checkServer", () => {
  it("accepts a Saldo server that speaks API v1", async () => {
    const fetch = health({ status: "ok", app: "saldo", api: "v1", version: "0.1.0" });
    expect(await checkServer("https://money.example.com")).toEqual({
      ok: true,
      version: "0.1.0",
    });
    expect((fetch.mock.calls[0] as unknown as [Request])[0].url).toBe(
      "https://money.example.com/healthz",
    );
  });

  it("explains what is wrong otherwise", async () => {
    health({ status: "ok" });
    expect(await checkServer("https://x.example")).toEqual({
      ok: false,
      reason: "This address does not answer like a Saldo server.",
    });
    health({ status: "ok", app: "saldo", api: "v2", version: "9.0.0" });
    expect(await checkServer("https://x.example")).toMatchObject({ ok: false });
    health({ status: "unavailable", app: "saldo", api: "v1", version: "0.1.0" }, 503);
    expect(await checkServer("https://x.example")).toMatchObject({ ok: false });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Failed to fetch");
      }),
    );
    expect(await checkServer("https://x.example")).toEqual({
      ok: false,
      reason: "The server cannot be reached.",
    });
  });
});
