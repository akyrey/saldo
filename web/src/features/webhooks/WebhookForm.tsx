import { useState } from "react";
import { fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EVENT_TYPE_HINTS, parseEventTypes } from "./event-types";
import type { WebhookInput } from "./queries";

/** The new-endpoint form. Server field errors appear under their field. */
export function WebhookForm({
  onSubmit,
  busy,
  error,
}: {
  onSubmit: (input: WebhookInput) => void;
  busy: boolean;
  error: unknown;
}) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [types, setTypes] = useState("*");
  const [imports, setImports] = useState(false);
  const errors = fieldErrors(error);

  return (
    <form
      className="grid gap-3 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({
          name: name.trim(),
          url: url.trim(),
          event_types: parseEventTypes(types),
          include_import_events: imports,
        });
      }}
    >
      <Field id="webhook-name" label="Name" error={errors.name}>
        <Input
          id="webhook-name"
          required
          value={name}
          aria-invalid={Boolean(errors.name)}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      <Field id="webhook-url" label="URL" error={errors.url}>
        <Input
          id="webhook-url"
          type="url"
          required
          placeholder="http://homeassistant.local:8123/api/webhook/…"
          value={url}
          aria-invalid={Boolean(errors.url)}
          onChange={(e) => setUrl(e.target.value)}
        />
      </Field>
      <Field
        id="webhook-types"
        label="Event types"
        error={errors.event_types}
        className="sm:col-span-2"
      >
        <Input
          id="webhook-types"
          value={types}
          aria-invalid={Boolean(errors.event_types)}
          aria-describedby="webhook-types-hint"
          onChange={(e) => setTypes(e.target.value)}
        />
        <p id="webhook-types-hint" className="text-xs text-muted-foreground">
          Comma-separated, for example {EVENT_TYPE_HINTS.join(", ")}.
        </p>
      </Field>
      <label className="flex items-center gap-2 text-sm sm:col-span-2">
        <input type="checkbox" checked={imports} onChange={(e) => setImports(e.target.checked)} />
        Also send events from the Firefly importer
      </label>
      <div className="sm:col-span-2">
        <Button type="submit" disabled={busy}>
          Add webhook
        </Button>
      </div>
    </form>
  );
}
