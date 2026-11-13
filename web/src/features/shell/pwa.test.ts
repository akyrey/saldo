import { describe, expect, it } from "vitest";
import { installHint, overdueCount } from "./pwa";

const IPHONE =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
const IPAD_DESKTOP_MODE =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
const ANDROID_CHROME =
  "Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Mobile Safari/537.36";
const IOS_CHROME =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/140.0 Mobile/15E148 Safari/604.1";

describe("installHint", () => {
  it("offers the browser's install prompt when there is one", () => {
    expect(
      installHint({
        userAgent: ANDROID_CHROME,
        touchPoints: 5,
        standalone: false,
        canPrompt: true,
      }),
    ).toBe("prompt");
  });

  it("explains Add to Home Screen in Safari on iPhone and iPad", () => {
    expect(
      installHint({ userAgent: IPHONE, touchPoints: 5, standalone: false, canPrompt: false }),
    ).toBe("ios");
    // iPadOS Safari claims to be a Mac; touch support gives it away.
    expect(
      installHint({
        userAgent: IPAD_DESKTOP_MODE,
        touchPoints: 5,
        standalone: false,
        canPrompt: false,
      }),
    ).toBe("ios");
  });

  it("says nothing once installed, on desktop Safari, or in other iOS browsers", () => {
    expect(
      installHint({ userAgent: IPHONE, touchPoints: 5, standalone: true, canPrompt: false }),
    ).toBeNull();
    expect(
      installHint({
        userAgent: IPAD_DESKTOP_MODE,
        touchPoints: 0,
        standalone: false,
        canPrompt: false,
      }),
    ).toBeNull();
    expect(
      installHint({ userAgent: IOS_CHROME, touchPoints: 5, standalone: false, canPrompt: false }),
    ).toBeNull();
    expect(
      installHint({
        userAgent: ANDROID_CHROME,
        touchPoints: 5,
        standalone: false,
        canPrompt: false,
      }),
    ).toBeNull();
  });
});

describe("overdueCount", () => {
  const bill = (next: string | null, active = true) => ({ active, next_due_date: next });

  it("counts active bills whose earliest open period is before today", () => {
    const bills = [
      bill("2026-11-10"),
      bill("2026-11-13"),
      bill("2026-11-20"),
      bill(null),
      bill("2026-10-01", false),
    ];
    expect(overdueCount(bills, "2026-11-13")).toBe(1);
  });
});
