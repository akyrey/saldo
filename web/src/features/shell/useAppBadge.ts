import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { billsQuery } from "@/features/bills/queries";
import { overdueCount } from "./pwa";

/** The App Badging API, where the browser has it (installed PWAs). */
type BadgeNavigator = Navigator & {
  setAppBadge?: (count?: number) => Promise<void>;
  clearAppBadge?: () => Promise<void>;
};

/** Today in the device's time zone, as YYYY-MM-DD. */
function today(): string {
  return new Date().toLocaleDateString("en-CA");
}

/** Shows the number of overdue bills on the app icon where supported. */
export function useAppBadge(): void {
  const badges = navigator as BadgeNavigator;
  const supported = typeof badges.setAppBadge === "function";
  const bills = useQuery({ ...billsQuery, enabled: supported });
  const count = bills.data ? overdueCount(bills.data, today()) : 0;

  useEffect(() => {
    if (!supported) return;
    // A refused badge (permission, platform) is cosmetic: ignore it.
    const done = count > 0 ? badges.setAppBadge?.(count) : badges.clearAppBadge?.();
    void done?.catch(() => {});
  }, [badges, supported, count]);
}
