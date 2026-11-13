import { afterEach, describe, expect, it, vi } from "vitest";
import { base64UrlToBytes, PushDeniedError, pushSupported, subscribeBrowser } from "./push";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("base64UrlToBytes", () => {
  it("decodes unpadded base64url", () => {
    expect(Array.from(base64UrlToBytes("-_8"))).toEqual([0xfb, 0xff]);
    expect(base64UrlToBytes("AAAA").length).toBe(3);
  });
});

describe("subscribeBrowser", () => {
  function stubPush(permission: NotificationPermission, existing: unknown = null) {
    const json = { endpoint: "https://push.example.com/x", keys: { p256dh: "p", auth: "a" } };
    const subscribe = vi.fn(async () => ({ toJSON: () => json }));
    vi.stubGlobal("Notification", { requestPermission: vi.fn(async () => permission) });
    vi.stubGlobal("PushManager", function PushManager() {});
    Object.defineProperty(navigator, "serviceWorker", {
      configurable: true,
      value: {
        ready: Promise.resolve({
          pushManager: { getSubscription: vi.fn(async () => existing), subscribe },
        }),
      },
    });
    return { subscribe, json };
  }

  it("subscribes with the VAPID key and returns the subscription JSON", async () => {
    const { subscribe, json } = stubPush("granted");
    expect(pushSupported()).toBe(true);
    await expect(subscribeBrowser("AAAA")).resolves.toEqual(json);
    expect(subscribe).toHaveBeenCalledWith(
      expect.objectContaining({
        userVisibleOnly: true,
        applicationServerKey: expect.any(Uint8Array),
      }),
    );
  });

  it("reuses an existing subscription", async () => {
    const existing = { toJSON: () => ({ endpoint: "https://old.example.com" }) };
    const { subscribe } = stubPush("granted", existing);
    await expect(subscribeBrowser("AAAA")).resolves.toEqual({
      endpoint: "https://old.example.com",
    });
    expect(subscribe).not.toHaveBeenCalled();
  });

  it("fails clearly when permission is denied", async () => {
    stubPush("denied");
    await expect(subscribeBrowser("AAAA")).rejects.toBeInstanceOf(PushDeniedError);
  });
});
