import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { AddChannelForm } from "./AddChannelForm";
import { describeChannel } from "./channel-form";
import { pushSupported, subscribeBrowser } from "./push";
import { type Channel, channelsQuery, vapidKeyQuery } from "./queries";

/** Your own reminder channels: only you see them, and only you get these. */
export function NotificationsPage() {
  const channels = useQuery(channelsQuery);
  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <PageHeader title="Notifications" />
      <p className="text-sm text-muted-foreground">
        Bill reminders go to every active channel below. Only you see your channels.
      </p>
      <Card>
        <CardHeader>
          <CardTitle>Your channels</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <QueryState isPending={channels.isPending} error={channels.error} />
          {channels.data?.length === 0 ? (
            <p className="text-sm text-muted-foreground">No channels yet.</p>
          ) : null}
          {channels.data?.map((channel) => (
            <ChannelRow key={channel.id} channel={channel} />
          ))}
        </CardContent>
      </Card>
      <BrowserPushCard />
      <Card>
        <CardHeader>
          <CardTitle>Add a channel</CardTitle>
          <CardDescription>ntfy, Gotify or email.</CardDescription>
        </CardHeader>
        <CardContent>
          <AddChannelForm />
        </CardContent>
      </Card>
    </div>
  );
}

function ChannelRow({ channel }: { channel: Channel }) {
  const queryClient = useQueryClient();
  const path = { params: { path: { id: channel.id } } };
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["notification-channels"] });
  const toggle = useMutation({
    mutationFn: () =>
      call(
        api.PATCH("/api/v1/notification-channels/{id}", {
          ...path,
          body: { active: !channel.active },
        }),
      ),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: () => call(api.DELETE("/api/v1/notification-channels/{id}", path)),
    onSuccess: refresh,
  });
  const test = useMutation({
    mutationFn: async () =>
      (await call(api.POST("/api/v1/notification-channels/{id}/test", path))).data,
  });
  const failure = toggle.error ?? remove.error ?? test.error;
  return (
    <div className="flex flex-col gap-2 border-b pb-3 last:border-b-0" data-testid="channel">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{channel.name}</span>
        <Badge>{channel.kind}</Badge>
        {channel.active ? null : <Badge className="bg-muted text-muted-foreground">paused</Badge>}
        <div className="ml-auto flex gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={() => test.mutate()}
            disabled={test.isPending}
          >
            Send test
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => toggle.mutate()}
            disabled={toggle.isPending}
          >
            {channel.active ? "Pause" : "Resume"}
          </Button>
          <Button
            size="sm"
            variant="destructive"
            onClick={() => remove.mutate()}
            disabled={remove.isPending}
          >
            Delete
          </Button>
        </div>
      </div>
      <p className="break-all text-sm text-muted-foreground">{describeChannel(channel)}</p>
      {test.data ? (
        <p className="text-sm" role="status">
          {test.data.sent ? "Test sent." : `Test failed: ${test.data.error ?? "unknown error"}`}
        </p>
      ) : null}
      {failure ? <Alert>{errorMessage(failure)}</Alert> : null}
    </div>
  );
}

function BrowserPushCard() {
  const queryClient = useQueryClient();
  const vapidKey = useQuery(vapidKeyQuery);
  const [problem, setProblem] = useState<string | null>(null);
  const enable = useMutation({
    mutationFn: async (publicKey: string) => {
      const subscription = await subscribeBrowser(publicKey);
      return call(
        api.POST("/api/v1/notification-channels", {
          body: { kind: "webpush", name: browserName(), config: { ...subscription } },
        }),
      );
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["notification-channels"] }),
    onError: (error) =>
      setProblem(
        error instanceof Error && !("code" in error) ? error.message : errorMessage(error),
      ),
  });

  let body;
  if (vapidKey.isPending) body = null;
  else if (!vapidKey.data)
    body = <p className="text-sm text-muted-foreground">Web Push is not set up on this server.</p>;
  else if (!pushSupported())
    body = (
      <p className="text-sm text-muted-foreground">
        This browser cannot receive push notifications.
      </p>
    );
  else {
    const key = vapidKey.data;
    body = (
      <Button onClick={() => enable.mutate(key)} disabled={enable.isPending}>
        Enable notifications in this browser
      </Button>
    );
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>This browser</CardTitle>
        <CardDescription>
          Push notifications to this device, even when Saldo is closed.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        {body}
        {enable.isSuccess ? (
          <p className="text-sm" role="status">
            This browser now gets reminders.
          </p>
        ) : null}
        {problem && enable.isError ? <Alert>{problem}</Alert> : null}
      </CardContent>
    </Card>
  );
}

/** A default channel name such as "Firefox on Android". */
function browserName(): string {
  const agent = navigator.userAgent;
  const browser = /Firefox/.test(agent)
    ? "Firefox"
    : /Edg\//.test(agent)
      ? "Edge"
      : /Chrome/.test(agent)
        ? "Chrome"
        : /Safari/.test(agent)
          ? "Safari"
          : "Browser";
  const system = /Android/.test(agent)
    ? "Android"
    : /iPhone|iPad/.test(agent)
      ? "iOS"
      : /Mac/.test(agent)
        ? "macOS"
        : /Windows/.test(agent)
          ? "Windows"
          : /Linux/.test(agent)
            ? "Linux"
            : "";
  return system ? `${browser} on ${system}` : browser;
}
