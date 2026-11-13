// The install banner and the overdue-bills badge, inside the whole app.
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
  delete (navigator as { setAppBadge?: unknown }).setAppBadge;
  delete (navigator as { clearAppBadge?: unknown }).clearAppBadge;
});

function api(extra: Record<string, unknown> = {}) {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/accounts/balances": list([]),
    ...extra,
  });
}

describe("install banner", () => {
  it("offers the browser's install prompt and can be dismissed for good", async () => {
    api();
    renderApp("/accounts");
    await screen.findByRole("navigation", { name: "Main" });
    expect(screen.queryByRole("region", { name: "Install Saldo" })).toBeNull();

    const prompt = vi.fn(async () => {});
    act(() => {
      window.dispatchEvent(Object.assign(new Event("beforeinstallprompt"), { prompt }));
    });
    await screen.findByRole("region", { name: "Install Saldo" });
    await userEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(prompt).toHaveBeenCalled();
    // The browser's own dialog takes over; the banner goes away.
    expect(screen.queryByRole("region", { name: "Install Saldo" })).toBeNull();

    act(() => {
      window.dispatchEvent(Object.assign(new Event("beforeinstallprompt"), { prompt }));
    });
    await screen.findByRole("region", { name: "Install Saldo" });

    await userEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("region", { name: "Install Saldo" })).toBeNull();
    expect(localStorage.getItem("saldo.installDismissed")).toBe("1");
  });
});

describe("app badge", () => {
  it("shows how many bills are overdue", async () => {
    const setAppBadge = vi.fn(async () => {});
    Object.assign(navigator, { setAppBadge, clearAppBadge: vi.fn(async () => {}) });
    api({
      "GET /api/v1/bills": list([
        { id: "b1", name: "Rent", active: true, next_due_date: "2000-01-01" },
        { id: "b2", name: "Gym", active: true, next_due_date: "2999-01-01" },
      ]),
    });
    renderApp("/accounts");
    await waitFor(() => expect(setAppBadge).toHaveBeenCalledWith(1));
  });
});
