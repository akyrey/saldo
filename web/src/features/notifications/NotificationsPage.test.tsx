// The notification settings page inside the whole app, against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { apiError, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
});

const PHONE = {
  id: "c1",
  user_id: "u1",
  kind: "ntfy",
  name: "Phone",
  active: true,
  settings: { server_url: "https://ntfy.sh", topic: "bills", has_token: false },
  created_at: "",
  updated_at: "",
};

function api(routes: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/notification-channels": { data: [PHONE] },
    "GET /api/v1/push/vapid-key": { data: { public_key: null } },
    ...routes,
  });
}

describe("notifications page", () => {
  it("lists channels and sends a test", async () => {
    const seen = api({
      "POST /api/v1/notification-channels/c1/test": {
        data: { sent: false, error: "the server answered HTTP 401" },
      },
    });
    renderApp("/settings/notifications");

    const row = await screen.findByTestId("channel");
    expect(within(row).getByText("https://ntfy.sh/bills")).toBeInTheDocument();
    await userEvent.click(within(row).getByRole("button", { name: "Send test" }));
    expect(await within(row).findByRole("status")).toHaveTextContent(
      "Test failed: the server answered HTTP 401",
    );
    expect(seen.some((r) => r.method === "POST" && r.url.endsWith("/c1/test"))).toBe(true);
    expect(screen.getByText(/web push is not set up/i)).toBeInTheDocument();
  });

  it("pauses and deletes a channel", async () => {
    const seen = api({
      "PATCH /api/v1/notification-channels/c1": { data: { ...PHONE, active: false } },
      "DELETE /api/v1/notification-channels/c1": () => new Response(null, { status: 204 }),
    });
    renderApp("/settings/notifications");

    const row = await screen.findByTestId("channel");
    await userEvent.click(within(row).getByRole("button", { name: "Pause" }));
    await userEvent.click(within(row).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(seen.some((r) => r.method === "DELETE")).toBe(true));
    const patch = seen.find((r) => r.method === "PATCH");
    expect(await patch?.json()).toEqual({ active: false });
  });

  it("adds an ntfy channel and shows field errors from the server", async () => {
    let attempts = 0;
    const seen = api({
      "GET /api/v1/notification-channels": { data: [] },
      "POST /api/v1/notification-channels": () => {
        attempts += 1;
        if (attempts === 1) {
          return new Response(
            JSON.stringify({
              error: {
                code: "validation_failed",
                message: "invalid",
                details: [
                  { field: "config.topic", code: "invalid", message: "must be 1 to 64 letters" },
                ],
              },
            }),
            { status: 422, headers: { "Content-Type": "application/json" } },
          );
        }
        return new Response(JSON.stringify({ data: PHONE }), {
          status: 201,
          headers: { "Content-Type": "application/json" },
        });
      },
    });
    renderApp("/settings/notifications");

    expect(await screen.findByText("No channels yet.")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Name"), "Phone");
    await userEvent.type(screen.getByLabelText("Topic"), "bad topic");
    await userEvent.click(screen.getByRole("button", { name: "Add channel" }));
    expect(await screen.findByTestId("topic-error")).toHaveTextContent("must be 1 to 64 letters");

    await userEvent.clear(screen.getByLabelText("Topic"));
    await userEvent.type(screen.getByLabelText("Topic"), "bills");
    await userEvent.click(screen.getByRole("button", { name: "Add channel" }));
    await waitFor(() => expect(attempts).toBe(2));
    const posts = seen.filter((r) => r.method === "POST");
    expect(await posts[1]?.json()).toEqual({
      kind: "ntfy",
      name: "Phone",
      config: { server_url: "https://ntfy.sh", topic: "bills" },
    });
  });

  it("switches the form fields with the kind", async () => {
    api({
      "POST /api/v1/notification-channels": apiError(
        422,
        "validation_failed",
        "email is not configured",
      ),
    });
    renderApp("/settings/notifications");

    await userEvent.selectOptions(await screen.findByLabelText("Kind"), "email");
    expect(screen.queryByLabelText("Topic")).not.toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Send to"), "me@example.com");
    await userEvent.click(screen.getByRole("button", { name: "Add channel" }));
    expect(await screen.findByText("email is not configured")).toBeInTheDocument();
  });

  it("subscribes this browser when the server has a VAPID key", async () => {
    const subscription = {
      endpoint: "https://push.example.com/x",
      keys: { p256dh: "p", auth: "a" },
    };
    vi.stubGlobal("Notification", { requestPermission: vi.fn(async () => "granted") });
    vi.stubGlobal("PushManager", function PushManager() {});
    Object.defineProperty(navigator, "serviceWorker", {
      configurable: true,
      value: {
        ready: Promise.resolve({
          pushManager: {
            getSubscription: vi.fn(async () => null),
            subscribe: vi.fn(async () => ({ toJSON: () => subscription })),
          },
        }),
      },
    });
    const seen = api({
      "GET /api/v1/push/vapid-key": { data: { public_key: "AAAA" } },
      "POST /api/v1/notification-channels": { data: { ...PHONE, kind: "webpush" } },
    });
    renderApp("/settings/notifications");

    await userEvent.click(
      await screen.findByRole("button", { name: /enable notifications in this browser/i }),
    );
    expect(await screen.findByText(/this browser now gets reminders/i)).toBeInTheDocument();
    const post = seen.find((r) => r.method === "POST");
    const body = await post?.json();
    expect(body.kind).toBe("webpush");
    expect(body.config).toEqual(subscription);
  });
});
