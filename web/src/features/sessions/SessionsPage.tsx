import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import type { components } from "@/api/schema";
import { PageHeader } from "@/components/PageHeader";
import { QueryState } from "@/components/QueryState";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

type Session = components["schemas"]["SessionView"];

const SESSIONS_KEY = ["auth-sessions"];

const sessionsQuery = queryOptions({
  queryKey: SESSIONS_KEY,
  queryFn: async (): Promise<Session[]> => (await call(api.GET("/api/v1/auth/sessions"))).data,
});

const CLIENT_LABELS: Record<NonNullable<Session["client"]>, string> = {
  browser: "Web browser",
  desktop: "Desktop app",
};

function clientLabel(client: Session["client"]): string {
  return client ? CLIENT_LABELS[client] : "Unknown app";
}

function when(timestamp: string): string {
  return new Date(timestamp).toLocaleString();
}

/** Where you are logged in; sign out a lost phone or every other device. */
export function SessionsPage() {
  const sessions = useQuery(sessionsQuery);
  const hasOthers = sessions.data?.some((s) => !s.current) ?? false;
  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <PageHeader title="Sessions" />
      <p className="text-sm text-muted-foreground">
        Every browser and desktop app you are logged in with. Personal access tokens are not listed
        here and stay valid.
      </p>
      <Card>
        <CardHeader>
          <CardTitle>Your logins</CardTitle>
          <CardDescription>A login lasts 30 days.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <QueryState isPending={sessions.isPending} error={sessions.error} />
          {sessions.data?.map((session) => (
            <SessionRow key={session.id} session={session} />
          ))}
        </CardContent>
      </Card>
      <SignOutOthers hasOthers={hasOthers} />
    </div>
  );
}

function SessionRow({ session }: { session: Session }) {
  const queryClient = useQueryClient();
  const end = useMutation({
    mutationFn: () =>
      call(api.DELETE("/api/v1/auth/sessions/{id}", { params: { path: { id: session.id } } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SESSIONS_KEY }),
  });
  const lastUsed = session.last_used_at ? `Last used ${when(session.last_used_at)}` : "Never used";
  return (
    <div className="flex flex-col gap-1 border-b pb-3 last:border-b-0" data-testid="session">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{clientLabel(session.client)}</span>
        {session.current ? <Badge>This device</Badge> : null}
        {session.current ? null : (
          <Button
            size="sm"
            variant="outline"
            className="ml-auto"
            onClick={() => end.mutate()}
            disabled={end.isPending}
          >
            Sign out
          </Button>
        )}
      </div>
      <p className="text-sm text-muted-foreground">
        {lastUsed} · logged in {when(session.created_at)} · expires {when(session.expires_at)}
      </p>
      {end.error ? <Alert>{errorMessage(end.error)}</Alert> : null}
    </div>
  );
}

/** Hidden until there is another login to end; stays to show the result. */
function SignOutOthers({ hasOthers }: { hasOthers: boolean }) {
  const queryClient = useQueryClient();
  const revoke = useMutation({
    mutationFn: async () => (await call(api.POST("/api/v1/auth/sessions/revoke-others"))).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SESSIONS_KEY }),
  });
  if (!hasOthers && !revoke.data) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle>Lost a device?</CardTitle>
        <CardDescription>Sign out every login except this one.</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        {hasOthers ? (
          <Button
            variant="destructive"
            className="self-start"
            onClick={() => revoke.mutate()}
            disabled={revoke.isPending}
          >
            Sign out everywhere else
          </Button>
        ) : null}
        {revoke.data ? (
          <p className="text-sm" role="status">
            Signed out {revoke.data.revoked} other{" "}
            {revoke.data.revoked === 1 ? "session" : "sessions"}.
          </p>
        ) : null}
        {revoke.error ? <Alert>{errorMessage(revoke.error)}</Alert> : null}
      </CardContent>
    </Card>
  );
}
