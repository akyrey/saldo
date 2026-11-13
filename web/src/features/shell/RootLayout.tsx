import { Outlet } from "@tanstack/react-router";
import { OfflineBanner } from "./OfflineBanner";

export function RootLayout() {
  return (
    <div className="min-h-dvh">
      <OfflineBanner />
      <Outlet />
    </div>
  );
}
