import { errorMessage } from "@/api/errors";
import { Alert } from "@/components/ui/alert";

/** Loading and error placeholders for a query; renders nothing once loaded. */
export function QueryState({ isPending, error }: { isPending: boolean; error: unknown }) {
  if (error) return <Alert>{errorMessage(error)}</Alert>;
  if (isPending) return <p className="text-sm text-muted-foreground">Loading…</p>;
  return null;
}
