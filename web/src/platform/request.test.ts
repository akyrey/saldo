import { describe, expect, it } from "vitest";
import { toServerRequest } from "./request";

describe("toServerRequest", () => {
  it("leaves browser requests alone", async () => {
    const request = new Request("http://localhost/api/v1/accounts");
    expect(await toServerRequest(request, { serverUrl: "", token: null })).toBe(request);
  });

  it("sends desktop requests to the server with the bearer token and no cookies", async () => {
    const request = new Request("tauri://localhost/api/v1/accounts?page=2", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Saldo-Csrf": "1" },
      body: JSON.stringify({ name: "Checking" }),
    });

    const sent = await toServerRequest(request, {
      serverUrl: "https://money.example.com",
      token: "secret",
    });

    expect(sent.url).toBe("https://money.example.com/api/v1/accounts?page=2");
    expect(sent.method).toBe("POST");
    expect(sent.credentials).toBe("omit");
    expect(sent.headers.get("Authorization")).toBe("Bearer secret");
    expect(sent.headers.get("Content-Type")).toBe("application/json");
    // A custom header would need its own CORS allowance; bearer requests skip CSRF.
    expect(sent.headers.get("X-Saldo-Csrf")).toBeNull();
    expect(await sent.json()).toEqual({ name: "Checking" });
  });

  it("sends no Authorization header before login", async () => {
    const request = new Request("tauri://localhost/api/v1/auth/login", { method: "POST" });
    const sent = await toServerRequest(request, {
      serverUrl: "https://money.example.com",
      token: null,
    });
    expect(sent.headers.get("Authorization")).toBeNull();
  });
});
