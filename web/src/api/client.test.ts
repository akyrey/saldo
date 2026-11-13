import { afterEach, describe, expect, it, vi } from "vitest";
import { call, createApi, setUnauthorizedListener } from "./client";
import { ApiError, errorMessage, fieldErrors } from "./errors";

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  setUnauthorizedListener(() => {});
});

describe("api client", () => {
  it("sends the CSRF header and uses the base URL", async () => {
    const fetchMock = vi.fn((_request: Request) =>
      Promise.resolve(jsonResponse(200, { data: { needed: false } })),
    );
    vi.stubGlobal("fetch", fetchMock);
    const api = createApi("https://saldo.example");

    const body = await call(api.GET("/api/v1/setup"));

    expect(body).toEqual({ data: { needed: false } });
    const request = fetchMock.mock.calls[0]?.[0];
    expect(request?.url).toBe("https://saldo.example/api/v1/setup");
    expect(request?.headers.get("X-Saldo-Csrf")).toBe("1");
  });

  it("turns the error envelope into an ApiError with field details", async () => {
    const envelope = {
      error: {
        code: "validation_failed",
        message: "invalid",
        details: [
          { field: "splits[1].amount", code: "amount_not_positive", message: "must be > 0" },
        ],
      },
    };
    vi.stubGlobal("fetch", () => Promise.resolve(jsonResponse(422, envelope)));
    const api = createApi("http://localhost");

    const error = await call(api.POST("/api/v1/transactions", { body: { splits: [] } })).catch(
      (e: unknown) => e,
    );

    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).status).toBe(422);
    expect(fieldErrors(error)).toEqual({ "splits[1].amount": "must be > 0" });
  });

  it("notifies the 401 listener except on login", async () => {
    const listener = vi.fn();
    setUnauthorizedListener(listener);
    const unauthorized = { error: { code: "unauthorized", message: "no", details: [] } };
    vi.stubGlobal("fetch", () => Promise.resolve(jsonResponse(401, unauthorized)));
    const api = createApi("http://localhost");

    await call(api.GET("/api/v1/auth/me")).catch(() => undefined);
    expect(listener).toHaveBeenCalledTimes(1);
    await call(api.POST("/api/v1/auth/login", { body: { email: "a", password: "b" } })).catch(
      () => undefined,
    );
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("reports a network failure as code network", async () => {
    vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
    const api = createApi("http://localhost");

    const error = await call(api.GET("/api/v1/setup")).catch((e: unknown) => e);

    expect((error as ApiError).code).toBe("network");
    expect(errorMessage(error)).toMatch(/offline/);
  });

  it("describes unknown errors generically", () => {
    expect(errorMessage(new Error("x"))).toBe("Something went wrong.");
    expect(fieldErrors("nope")).toEqual({});
  });
});
