// The webhook settings page inside the whole app, against a fake API.
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { apiError, CURRENCIES, fakeApi, json, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const HOOK = {
  id: "w1",
  name: "Home Assistant",
  url: "http://ha.local/api/webhook/x",
  event_types: ["transaction.*"],
  include_import_events: false,
  active: true,
  consecutive_failures: 0,
  disabled_reason: null,
  created_at: "",
  updated_at: "",
};

const DELIVERY = {
  id: "d1",
  endpoint_id: "w1",
  event_seq: 7,
  event_id: "e7",
  event_type: "transaction.created",
  status: "dead",
  attempts: 8,
  next_attempt_at: null,
  last_attempt_at: "2026-11-13T09:00:00.000Z",
  last_status_code: 500,
  last_error: "HTTP 500 Internal Server Error",
  created_at: "",
};

function api(me: unknown, routes: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": me,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/accounts/balances": list([]),
    "GET /api/v1/webhooks": { data: [HOOK] },
    ...routes,
  });
}

describe("webhooks page", () => {
  it("creates an endpoint and shows its secret once", async () => {
    const seen = api(ME, {
      "POST /api/v1/webhooks": () =>
        json(201, { data: { webhook: { ...HOOK, id: "w2", name: "n8n" }, secret: "whsec_abc" } }),
    });
    renderApp("/webhooks");
    await userEvent.type(await screen.findByLabelText("Name"), "n8n");
    await userEvent.type(screen.getByLabelText("URL"), "http://n8n.local/hook");
    const types = screen.getByLabelText("Event types");
    await userEvent.clear(types);
    await userEvent.type(types, "transaction.*, bill.overdue");
    await userEvent.click(screen.getByRole("button", { name: "Add webhook" }));

    expect(await screen.findByTestId("webhook-secret")).toHaveTextContent("whsec_abc");
    const post = seen.find((r) => r.method === "POST");
    expect(await post?.json()).toEqual({
      name: "n8n",
      url: "http://n8n.local/hook",
      event_types: ["transaction.*", "bill.overdue"],
      include_import_events: false,
    });
  });

  it("shows server field errors under the field", async () => {
    api(ME, {
      "POST /api/v1/webhooks": () =>
        json(422, {
          error: {
            code: "validation_failed",
            message: "validation failed",
            details: [
              { field: "url", code: "invalid", message: "must be an absolute http or https URL" },
            ],
          },
        }),
    });
    renderApp("/webhooks");
    await userEvent.type(await screen.findByLabelText("Name"), "x");
    await userEvent.type(screen.getByLabelText("URL"), "http://x.local");
    await userEvent.click(screen.getByRole("button", { name: "Add webhook" }));
    expect(await screen.findByTestId("webhook-url-error")).toHaveTextContent(/absolute http/);
  });

  it("lists deliveries and redelivers one", async () => {
    const seen = api(ME, {
      "GET /api/v1/webhooks/w1/deliveries": list([DELIVERY]),
      "POST /api/v1/webhook-deliveries/d1/redeliver": { data: { ...DELIVERY, status: "pending" } },
    });
    renderApp("/webhooks");
    expect(await screen.findByText("Home Assistant")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show deliveries" }));
    expect(await screen.findByText("dead")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /redeliver transaction.created/i }));
    await waitFor(() =>
      expect(seen.some((r) => new URL(r.url).pathname.endsWith("/d1/redeliver"))).toBe(true),
    );
  });

  it("sends a test event and toggles the endpoint", async () => {
    const seen = api(ME, {
      "POST /api/v1/webhooks/w1/test": () => json(202, { data: { event_id: "e9" } }),
      "PATCH /api/v1/webhooks/w1": { data: { ...HOOK, active: false } },
      "GET /api/v1/webhooks/w1/deliveries": list([]),
    });
    renderApp("/webhooks");
    await userEvent.click(await screen.findByRole("button", { name: "Send test" }));
    expect(await screen.findByText(/test event queued/i)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Deactivate" }));
    await waitFor(() => expect(seen.some((r) => r.method === "PATCH")).toBe(true));
    const patch = seen.find((r) => r.method === "PATCH");
    expect(await patch?.json()).toEqual({ active: false });
  });

  it("tells members that only the owner manages webhooks", async () => {
    api(
      { data: { ...ME.data, role: "member" } },
      { "GET /api/v1/webhooks": apiError(403, "forbidden") },
    );
    renderApp("/webhooks");
    expect(await screen.findByText(/only the household owner/i)).toBeInTheDocument();
  });
});
