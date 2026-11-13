import { Button } from "@/components/ui/button";

/** Shows a just-created or rotated secret; it is never shown again. */
export function SecretNotice({
  name,
  secret,
  onDone,
}: {
  name: string;
  secret: string;
  onDone: () => void;
}) {
  return (
    <div
      role="status"
      className="mb-4 rounded-lg border border-primary/40 bg-primary/5 px-4 py-3 text-sm"
    >
      <p className="mb-2 font-medium">Signing secret for {name}</p>
      <p className="mb-2 text-muted-foreground">
        Copy it now: it is shown only once. Receivers use it to verify the X-Saldo-Signature header.
      </p>
      <code className="block break-all rounded bg-muted px-2 py-1" data-testid="webhook-secret">
        {secret}
      </code>
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="outline"
          onClick={() => void navigator.clipboard?.writeText(secret)}
        >
          Copy
        </Button>
        <Button size="sm" variant="ghost" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  );
}
