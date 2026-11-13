import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeApi } from "@/test/fake-api";
import { setPlatformForTests, type Platform } from "@/platform";
import { browserPlatform } from "@/platform/browser";
import { alertBill } from "./bill-alerts";

afterEach(() => {
  vi.unstubAllGlobals();
  setPlatformForTests(null);
});

function desktop(): Platform & { notify: ReturnType<typeof vi.fn> } {
  const fake = { ...browserPlatform, kind: "desktop" as const, notify: vi.fn(async () => {}) };
  setPlatformForTests(fake);
  return fake;
}

const BILL = {
  id: "b1",
  name: "Rent",
  amount_min: "800.00",
  amount_max: "800.00",
  currency: "EUR",
  next_due_date: "2026-11-15",
};

const notice = (type: string) => ({ seq: 9, type, entity_type: "bill", entity_id: "b1" });

describe("alertBill", () => {
  it("shows a system notification for a due bill on the desktop", async () => {
    fakeApi({ "GET /api/v1/bills/b1": { data: BILL } });
    const platform = desktop();
    await alertBill(notice("bill.due_soon"));
    expect(platform.notify).toHaveBeenCalledWith(
      "Bill due soon: Rent",
      "800.00 EUR due on 2026-11-15.",
    );
  });

  it("says an overdue bill is overdue, with its amount range", async () => {
    fakeApi({
      "GET /api/v1/bills/b1": { data: { ...BILL, amount_min: "29.90", amount_max: "35.00" } },
    });
    const platform = desktop();
    await alertBill(notice("bill.overdue"));
    expect(platform.notify).toHaveBeenCalledWith(
      "Bill overdue: Rent",
      "29.90–35.00 EUR is overdue.",
    );
  });

  it("ignores other events and the browser (which uses Web Push)", async () => {
    const seen = fakeApi({ "GET /api/v1/bills/b1": { data: BILL } });
    const platform = desktop();
    await alertBill(notice("bill.updated"));
    expect(platform.notify).not.toHaveBeenCalled();
    setPlatformForTests(null);
    await alertBill(notice("bill.due_soon"));
    expect(seen).toHaveLength(0);
  });
});
