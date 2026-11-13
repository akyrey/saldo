import { WifiOff } from "lucide-react";
import { useOnline } from "./useOnline";

/** Saldo keeps no data offline; say so instead of showing broken requests. */
export function OfflineBanner() {
  const online = useOnline();
  if (online) return null;
  return (
    <div
      role="status"
      className="flex items-center justify-center gap-2 bg-amber-100 px-4 py-2 text-sm text-amber-900"
    >
      <WifiOff className="size-4" aria-hidden />
      You are offline. Saldo needs a connection to load or save data.
    </div>
  );
}
