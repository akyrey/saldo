import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouteContext } from "@tanstack/react-router";
import { useState } from "react";
import { errorMessage, fieldErrors } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { createWebhook, webhooksQuery, type WebhookWithSecret } from "./queries";
import { SecretNotice } from "./SecretNotice";
import { WebhookCard } from "./WebhookCard";
import { WebhookForm } from "./WebhookForm";

/** Owner-only settings page: webhook endpoints and their delivery logs. */
export function WebhooksPage() {
  const { me } = useRouteContext({ from: "/app" });
  if (me.role !== "owner") {
    return (
      <div className="max-w-3xl">
        <PageHeader title="Webhooks" />
        <Alert>Only the household owner can manage webhooks.</Alert>
      </div>
    );
  }
  return <OwnerWebhooks />;
}

function OwnerWebhooks() {
  const queryClient = useQueryClient();
  const webhooks = useQuery(webhooksQuery);
  const [secret, setSecret] = useState<WebhookWithSecret | null>(null);
  const [formKey, setFormKey] = useState(0);
  const create = useMutation({
    mutationFn: createWebhook,
    onSuccess: async (created) => {
      setSecret(created);
      // A new key resets the form's fields.
      setFormKey((key) => key + 1);
      await queryClient.invalidateQueries({ queryKey: ["webhooks"] });
    },
  });

  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <PageHeader title="Webhooks" />
      <p className="text-sm text-muted-foreground">
        Saldo POSTs every matching event as signed JSON to these URLs, retrying for about two days.
        See docs/WEBHOOKS.md for the payload and how to check the signature.
      </p>
      {secret ? (
        <SecretNotice
          name={secret.webhook.name}
          secret={secret.secret}
          onDone={() => setSecret(null)}
        />
      ) : null}
      <Card>
        <CardHeader>
          <CardTitle>New webhook</CardTitle>
        </CardHeader>
        <CardContent>
          {create.error && Object.keys(fieldErrors(create.error)).length === 0 ? (
            <Alert className="mb-3">{errorMessage(create.error)}</Alert>
          ) : null}
          <WebhookForm
            key={formKey}
            busy={create.isPending}
            error={create.error}
            onSubmit={(input) => create.mutate(input)}
          />
        </CardContent>
      </Card>
      <QueryState isPending={webhooks.isPending} error={webhooks.error} />
      {webhooks.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No webhooks yet.</p>
      ) : null}
      {webhooks.data?.map((webhook) => (
        <WebhookCard key={webhook.id} webhook={webhook} onSecret={setSecret} />
      ))}
    </div>
  );
}
