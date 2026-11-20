// Desktop updates: the app checks once at start, offers a newer version and
// installs it; "More > Check for updates" asks again by hand.
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setPlatformForTests, type AppUpdate, type Platform } from "@/platform";
import { browserPlatform } from "@/platform/browser";
import { CURRENCIES, fakeApi, list, ME, MEMBERS } from "@/test/fake-api";
import { renderApp } from "@/test/render-app";

afterEach(() => {
  vi.unstubAllGlobals();
  setPlatformForTests(null);
});

function api() {
  fakeApi({
    "GET /api/v1/auth/me": ME,
    "GET /api/v1/household/members": MEMBERS,
    "GET /api/v1/currencies": CURRENCIES,
    "GET /api/v1/accounts": list([]),
    "GET /api/v1/accounts/balances": list([]),
  });
}

function fakeUpdate(install: AppUpdate["install"] = async () => {}): AppUpdate {
  return { version: "0.3.0", currentVersion: "0.2.0", install: vi.fn(install) };
}

function desktop(checkForUpdate: Platform["checkForUpdate"]) {
  const fake: Platform = {
    ...browserPlatform,
    kind: "desktop",
    serverUrl: () => "https://money.example.com",
    token: () => "desktop-token",
    checkForUpdate: vi.fn(checkForUpdate),
    setTheme: async () => {},
  };
  setPlatformForTests(fake);
  return fake;
}

/** Opens "More" in the header (the phone bar has its own). */
async function openMore() {
  const header = screen.getByRole("navigation", { name: "Main" });
  await userEvent.click(within(header).getByRole("button", { name: "More" }));
}

async function checkByHand() {
  await openMore();
  await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
}

describe("desktop updates", () => {
  it("offers a newer version at start and installs it with progress", async () => {
    let finish: () => void = () => {};
    const update = fakeUpdate(
      (onProgress) =>
        new Promise<void>((resolve) => {
          onProgress(0.4);
          finish = resolve;
        }),
    );
    desktop(async () => update);
    api();
    renderApp("/accounts");

    const banner = await screen.findByRole("region", { name: "Update" });
    expect(banner).toHaveTextContent("Saldo 0.3.0 is available (you have 0.2.0).");
    await userEvent.click(within(banner).getByRole("button", { name: "Update and restart" }));

    expect(update.install).toHaveBeenCalledOnce();
    await within(banner).findByText("Downloading the update… 40%");
    expect(within(banner).queryByRole("button")).toBeNull();
    finish();
  });

  it("hides the offer after Later", async () => {
    desktop(async () => fakeUpdate());
    api();
    renderApp("/accounts");

    const banner = await screen.findByRole("region", { name: "Update" });
    await userEvent.click(within(banner).getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("region", { name: "Update" })).toBeNull();
  });

  it("says nothing at start when up to date, but says so when asked", async () => {
    const fake = desktop(async () => null);
    api();
    renderApp("/accounts");
    await screen.findByRole("navigation", { name: "Main" });
    await waitFor(() => expect(fake.checkForUpdate).toHaveBeenCalledOnce());
    expect(screen.queryByRole("region", { name: "Update" })).toBeNull();

    await checkByHand();
    const banner = await screen.findByRole("region", { name: "Update" });
    expect(banner).toHaveTextContent("Saldo is up to date.");
    expect(fake.checkForUpdate).toHaveBeenCalledTimes(2);
  });

  it("keeps a failed start-up check quiet but shows a failed manual one", async () => {
    desktop(async () => {
      throw new Error("offline");
    });
    api();
    renderApp("/accounts");
    await screen.findByRole("navigation", { name: "Main" });
    expect(screen.queryByRole("region", { name: "Update" })).toBeNull();

    await checkByHand();
    const banner = await screen.findByRole("region", { name: "Update" });
    expect(banner).toHaveTextContent("Could not check for updates: offline");
  });

  it("shows a failed install and lets you try again", async () => {
    const update = fakeUpdate(async () => {
      throw new Error("signature mismatch");
    });
    desktop(async () => update);
    api();
    renderApp("/accounts");

    const banner = await screen.findByRole("region", { name: "Update" });
    await userEvent.click(within(banner).getByRole("button", { name: "Update and restart" }));
    await within(banner).findByText("The update failed: signature mismatch");
    await userEvent.click(within(banner).getByRole("button", { name: "Try again" }));
    expect(update.install).toHaveBeenCalledTimes(2);
  });

  it("does nothing in the browser", async () => {
    const check = vi.fn(async () => null);
    setPlatformForTests({ ...browserPlatform, checkForUpdate: check });
    api();
    renderApp("/accounts");
    await screen.findByRole("navigation", { name: "Main" });

    await openMore();
    expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
    expect(check).not.toHaveBeenCalled();
  });
});
