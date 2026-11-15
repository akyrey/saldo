import { useCallback, useEffect, useRef, useState } from "react";
import { platform, type AppUpdate } from "@/platform";

export type UpdateState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "up-to-date" }
  | { status: "available"; update: AppUpdate }
  | { status: "installing"; update: AppUpdate; progress: number | null }
  | { status: "check-failed"; message: string }
  | { status: "install-failed"; update: AppUpdate; message: string };

export interface AppUpdateControls {
  state: UpdateState;
  /** `null` outside the desktop app, which has nothing to update. */
  checkNow: (() => void) | null;
  install: (update: AppUpdate) => void;
  dismiss: () => void;
}

const IDLE: UpdateState = { status: "idle" };

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Checks for a desktop update once at start; `checkNow` asks again by hand. */
export function useAppUpdate(): AppUpdateControls {
  const desktop = platform().kind === "desktop";
  const [state, setState] = useState<UpdateState>(IDLE);
  const checkedAtStart = useRef(false);

  const check = useCallback(async (manual: boolean) => {
    if (manual) setState({ status: "checking" });
    try {
      const update = await platform().checkForUpdate();
      if (update) setState({ status: "available", update });
      else if (manual) setState({ status: "up-to-date" });
    } catch (error) {
      if (manual) {
        setState({ status: "check-failed", message: messageOf(error) });
        return;
      }
      // At start, a failed check (offline, GitHub unreachable) is not worth a
      // banner: the next start or "Check for updates" tries again.
      console.warn("Saldo could not check for updates:", error);
    }
  }, []);

  useEffect(() => {
    // The ref keeps React's development double-run from checking twice.
    if (!desktop || checkedAtStart.current) return;
    checkedAtStart.current = true;
    void check(false);
  }, [desktop, check]);

  const install = useCallback((update: AppUpdate) => {
    setState({ status: "installing", update, progress: null });
    const onProgress = (progress: number | null) =>
      setState({ status: "installing", update, progress });
    // On success the app restarts, so only a failure comes back here.
    update.install(onProgress).catch((error: unknown) => {
      setState({ status: "install-failed", update, message: messageOf(error) });
    });
  }, []);

  return {
    state,
    checkNow: desktop ? () => void check(true) : null,
    install,
    dismiss: () => setState(IDLE),
  };
}
