import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { errorMessage } from "@/api/errors";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { DeliveryLog } from "./DeliveryLog";
import { formatEventTypes } from "./event-types";
import {
  deleteWebhook,
  rotateSecret,
  testWebhook,
  updateWebhook,
  type Webhook,
  type WebhookWithSecret,
} from "./queries";

/** One endpoint: its settings, actions and delivery log. */
export function WebhookCard({
  webhook,
  onSecret,
}: {
  webhook: Webhook;
  onSecret: (created: WebhookWithSecret) => void;
}) {
  const queryClient = useQueryClient();
  const [showLog, setShowLog] = useState(false);
  const [tested, setTested] = useState(false);
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["webhooks"] });
  const toggle = useMutation({
    mutationFn: () => updateWebhook(webhook.id, { active: !webhook.active }),
    onSuccess: refresh,
  });
  const test = useMutation({
    mutationFn: () => testWebhook(webhook.id),
    onSuccess: async () => {
      setTested(true);
      setShowLog(true);
      await refresh();
    },
  });
  const rotate = useMutation({
    mutationFn: () => rotateSecret(webhook.id),
    onSuccess: async (rotated) => {
      onSecret(rotated);
      await refresh();
    },
  });
  const remove = useMutation({ mutationFn: () => deleteWebhook(webhook.id), onSuccess: refresh });
  const error = toggle.error ?? test.error ?? rotate.error ?? remove.error;
  const busy = toggle.isPending || test.isPending || rotate.isPending || remove.isPending;

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-2">
          <CardTitle>{webhook.name}</CardTitle>
          <Badge className={webhook.active ? "" : "border-destructive/40 text-destructive"}>
            {webhook.active ? "active" : "inactive"}
          </Badge>
        </div>
        <CardDescription className="break-all">{webhook.url}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <p>
          <span className="text-muted-foreground">Events: </span>
          <span className="font-mono text-xs">{formatEventTypes(webhook.event_types)}</span>
          {webhook.include_import_events ? (
            <span className="text-muted-foreground"> (including imports)</span>
          ) : null}
        </p>
        {webhook.disabled_reason ? <Alert>{webhook.disabled_reason}</Alert> : null}
        {error ? <Alert>{errorMessage(error)}</Alert> : null}
        {tested ? (
          <p role="status" className="text-muted-foreground">
            Test event queued; it appears in the delivery log.
          </p>
        ) : null}
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" disabled={busy} onClick={() => test.mutate()}>
            Send test
          </Button>
          <Button size="sm" variant="outline" disabled={busy} onClick={() => toggle.mutate()}>
            {webhook.active ? "Deactivate" : "Activate"}
          </Button>
          <Button size="sm" variant="outline" disabled={busy} onClick={() => rotate.mutate()}>
            Rotate secret
          </Button>
          <Button size="sm" variant="outline" onClick={() => setShowLog((shown) => !shown)}>
            {showLog ? "Hide deliveries" : "Show deliveries"}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="text-destructive"
            disabled={busy}
            onClick={() => {
              if (window.confirm(`Delete the webhook ${webhook.name}?`)) remove.mutate();
            }}
          >
            Delete
          </Button>
        </div>
        {showLog ? <DeliveryLog webhookId={webhook.id} /> : null}
      </CardContent>
    </Card>
  );
}
