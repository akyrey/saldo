// The export page inside the whole app, against a fake API.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

const createObjectURL = vi.fn(() => "blob:saldo-test");
const revokeObjectURL = vi.fn();

beforeEach(() => {
  // jsdom has no object URLs; the page only needs them to exist.
  URL.createObjectURL = createObjectURL;
  URL.revokeObjectURL = revokeObjectURL;
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

function api(me: unknown) {
  return fakeApi({
    "GET /api/v1/auth/me": me,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/accounts/balances": list([]),
    "GET /api/v1/export/json": () =>
      new Response('{"format":"saldo-export"}', {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "Content-Disposition": 'attachment; filename="saldo-export-2026-11-13.json"',
        },
      }),
  });
}

describe("export page", () => {
  it("lets the owner download the JSON export", async () => {
    const seen = api(ME);
    const clicked = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    renderApp("/export");

    await userEvent.click(await screen.findByRole("button", { name: /download json/i }));

    await waitFor(() => expect(clicked).toHaveBeenCalledOnce());
    expect(seen.some((r) => new URL(r.url).pathname === "/api/v1/export/json")).toBe(true);
    expect(createObjectURL).toHaveBeenCalledOnce();
    const anchor = clicked.mock.contexts[0] as HTMLAnchorElement;
    expect(anchor.download).toBe("saldo-export-2026-11-13.json");
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:saldo-test");
  });

  it("shows members that only the owner can export", async () => {
    api({ data: { ...ME.data, role: "member" } });
    renderApp("/export");

    expect(await screen.findByText(/only the household owner/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /download/i })).not.toBeInTheDocument();
  });

  it("shows the export link in the navigation to owners only", async () => {
    api(ME);
    renderApp("/accounts");
    const nav = await screen.findByRole("navigation", { name: "Main" });
    await userEvent.click(within(nav).getByRole("button", { name: "More" }));
    expect(within(nav).getByRole("link", { name: "Export" })).toBeInTheDocument();
  });
});
