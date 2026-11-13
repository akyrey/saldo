import { Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { platform } from "@/platform";
import { installHint } from "./pwa";

/** Chrome's install event (not yet in TypeScript's DOM types). */
interface BeforeInstallPromptEvent extends Event {
  prompt(): Promise<void>;
}

const DISMISSED_KEY = "saldo.installDismissed";

function readDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

function standalone(): boolean {
  const iosStandalone = (navigator as Navigator & { standalone?: boolean }).standalone === true;
  return iosStandalone || window.matchMedia?.("(display-mode: standalone)").matches === true;
}

/** Offers to install Saldo as an app, then points to notification settings. */
export function InstallBanner() {
  const [deferred, setDeferred] = useState<BeforeInstallPromptEvent | null>(null);
  const [dismissed, setDismissed] = useState(readDismissed);

  useEffect(() => {
    const onPrompt = (event: Event) => {
      event.preventDefault(); // show our banner instead of the browser's mini bar
      setDeferred(event as BeforeInstallPromptEvent);
    };
    const onInstalled = () => setDeferred(null);
    window.addEventListener("beforeinstallprompt", onPrompt);
    window.addEventListener("appinstalled", onInstalled);
    return () => {
      window.removeEventListener("beforeinstallprompt", onPrompt);
      window.removeEventListener("appinstalled", onInstalled);
    };
  }, []);

  const hint = installHint({
    userAgent: navigator.userAgent,
    touchPoints: navigator.maxTouchPoints ?? 0,
    standalone: standalone(),
    canPrompt: deferred !== null,
  });
  if (dismissed || hint === null || platform().kind !== "browser") return null;

  const dismiss = () => {
    setDismissed(true);
    try {
      localStorage.setItem(DISMISSED_KEY, "1");
    } catch {
      // Shown again next visit; harmless.
    }
  };

  return (
    <section
      aria-label="Install Saldo"
      className="mb-4 flex flex-wrap items-center gap-3 rounded-md border bg-accent/40 p-3 text-sm"
    >
      <p className="flex-1">
        {hint === "ios"
          ? "Install Saldo: tap Share, then “Add to Home Screen”. Open it from the Home Screen to get bill reminders."
          : "Install Saldo as an app for quick access and bill reminders."}{" "}
        <Link to="/settings/notifications" className="text-primary underline">
          Notification settings
        </Link>
      </p>
      {hint === "prompt" && deferred ? (
        <Button
          size="sm"
          onClick={() => {
            void deferred.prompt();
            setDeferred(null);
          }}
        >
          Install
        </Button>
      ) : null}
      <Button size="sm" variant="ghost" onClick={dismiss}>
        Not now
      </Button>
    </section>
  );
}
