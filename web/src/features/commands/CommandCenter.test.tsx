import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { accountRoutes, CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => vi.unstubAllGlobals());

function api() {
  return fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    ...accountRoutes([], []),
    "GET /api/v1/categories": list([]),
    "GET /api/v1/tags": list([]),
    "GET /api/v1/budgets": list([]),
    "GET /api/v1/bills": list([]),
    "GET /api/v1/transactions": list([]),
    "GET /api/v1/saved-views": { data: [] },
    "GET /api/v1/transaction-templates": list([]),
  });
}

describe("CommandCenter", () => {
  it("follows g-sequences and n outside text fields", async () => {
    api();
    const { router } = renderApp("/transactions");
    const user = userEvent.setup();
    await screen.findByLabelText("Search");
    await user.keyboard("gb");
    await waitFor(() => expect(router.state.location.pathname).toBe("/budgets"));
    await user.keyboard("n");
    await waitFor(() => expect(router.state.location.pathname).toBe("/transactions/new"));
  });

  it("ignores shortcuts while typing", async () => {
    api();
    const { router } = renderApp("/transactions");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Search"), "gbn");
    expect(router.state.location.pathname).toBe("/transactions");
  });

  it("opens the palette with Ctrl+K and runs a page entry", async () => {
    api();
    const { router } = renderApp("/transactions");
    await screen.findByLabelText("Search");
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByPlaceholderText(/Go to a page/);
    const user = userEvent.setup();
    await user.type(input, "piggy");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(router.state.location.pathname).toBe("/piggy-banks"));
  });

  it("shows the shortcut list on ?", async () => {
    api();
    renderApp("/transactions");
    await screen.findByLabelText("Search");
    await userEvent.setup().keyboard("?");
    expect(await screen.findByText("Keyboard shortcuts")).toBeInTheDocument();
  });
});
