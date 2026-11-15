import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import type { AppUpdateControls } from "./useAppUpdate";

const PERCENT = 100;

function Banner({ children }: { children: ReactNode }) {
  return (
    <section
      aria-label="Update"
      aria-live="polite"
      className="mb-4 flex flex-wrap items-center gap-3 rounded-md border bg-accent/40 p-3 text-sm"
    >
      {children}
    </section>
  );
}

function progressText(progress: number | null): string {
  if (progress === null) return "Downloading the update…";
  return `Downloading the update… ${Math.round(progress * PERCENT)}%`;
}

/** The desktop app's update offer, progress and errors; nothing in the browser. */
export function UpdateBanner({ state, install, dismiss }: AppUpdateControls) {
  const close = (
    <Button size="sm" variant="ghost" onClick={dismiss}>
      Close
    </Button>
  );
  switch (state.status) {
    case "idle":
      return null;
    case "checking":
      return (
        <Banner>
          <p className="flex-1">Checking for updates…</p>
        </Banner>
      );
    case "up-to-date":
      return (
        <Banner>
          <p className="flex-1">Saldo is up to date.</p>
          {close}
        </Banner>
      );
    case "check-failed":
      return (
        <Banner>
          <p className="flex-1">Could not check for updates: {state.message}</p>
          {close}
        </Banner>
      );
    case "available":
      return (
        <Banner>
          <p className="flex-1">
            Saldo {state.update.version} is available (you have {state.update.currentVersion}).
          </p>
          <Button size="sm" onClick={() => install(state.update)}>
            Update and restart
          </Button>
          <Button size="sm" variant="ghost" onClick={dismiss}>
            Later
          </Button>
        </Banner>
      );
    case "installing":
      return (
        <Banner>
          <p className="flex-1">{progressText(state.progress)}</p>
        </Banner>
      );
    case "install-failed":
      return (
        <Banner>
          <p className="flex-1">The update failed: {state.message}</p>
          <Button size="sm" onClick={() => install(state.update)}>
            Try again
          </Button>
          {close}
        </Banner>
      );
  }
}
