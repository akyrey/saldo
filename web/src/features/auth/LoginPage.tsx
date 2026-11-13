import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { platform } from "@/platform";
import { AuthCard } from "./AuthCard";
import { meQuery, setupStatusQuery } from "./queries";

export function LoginPage() {
  const { redirect } = useSearch({ from: "/login" });
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const setupNeeded = useQuery(setupStatusQuery);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  const desktop = platform().kind === "desktop";
  const login = useMutation({
    mutationFn: async () => {
      // The desktop app asks for the token itself instead of a cookie.
      const client = desktop ? "desktop" : undefined;
      const result = await call(
        api.POST("/api/v1/auth/login", { body: { email, password, client } }),
      );
      if (result.data.token) await platform().saveToken(result.data.token);
    },
    onSuccess: async () => {
      await queryClient.fetchQuery(meQuery);
      await navigate({ to: redirect ?? "/accounts" });
    },
  });

  return (
    <AuthCard title="Log in" description="Use the email and password of your Saldo account.">
      {setupNeeded.data ? (
        <Alert className="mb-4 border-primary/40 bg-primary/5 text-foreground">
          No account exists yet.{" "}
          <Link to="/setup" className="font-medium text-primary underline">
            Set up Saldo
          </Link>
        </Alert>
      ) : null}
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          login.mutate();
        }}
      >
        <Field id="email" label="Email">
          <Input
            id="email"
            type="email"
            autoComplete="username"
            required
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        <Field id="password" label="Password">
          <Input
            id="password"
            type="password"
            autoComplete="current-password"
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>
        {login.error ? <Alert>{errorMessage(login.error)}</Alert> : null}
        <Button type="submit" disabled={login.isPending}>
          {login.isPending ? "Logging in…" : "Log in"}
        </Button>
      </form>
      {desktop ? (
        <p className="mt-4 text-sm text-muted-foreground">
          Server: {platform().serverUrl()} ·{" "}
          <Link to="/connect" className="text-primary underline">
            Change
          </Link>
        </p>
      ) : null}
    </AuthCard>
  );
}
