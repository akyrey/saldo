import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { useState } from "react";
import { api, call, oidcStartUrl } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { platform } from "@/platform";
import { AuthCard } from "./AuthCard";
import { authMethodsQuery, meQuery, setupStatusQuery } from "./queries";

/** Why "Log in with Authentik" sent the person back here (`?error=`). */
const PROVIDER_ERRORS: Record<string, string> = {
  oidc_no_account:
    "No Saldo member has this email. Ask the household owner to add you with the email you use at the identity provider.",
  oidc_expired: "The login took too long or was started elsewhere. Please try again.",
  oidc_refused: "The identity provider did not allow the login.",
  oidc_invalid: "The identity provider's answer could not be verified. Please try again.",
  oidc_unavailable: "The identity provider cannot be reached right now. Please try again later.",
  oidc_disabled: "Login with an identity provider is not set up on this server.",
};

export function LoginPage() {
  const { redirect, error } = useSearch({ from: "/login" });
  const methods = useQuery(authMethodsQuery);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const setupNeeded = useQuery(setupStatusQuery);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  const desktop = platform().kind === "desktop";
  // Until the server answers (or if it cannot), offer the password form.
  const passwordAllowed = methods.data?.password ?? true;
  // The desktop app cannot follow the provider's redirects yet.
  const provider = desktop ? null : (methods.data?.oidc ?? null);
  const providerError = error ? (PROVIDER_ERRORS[error] ?? "The login did not work.") : null;
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
      {providerError ? <Alert className="mb-4">{providerError}</Alert> : null}
      {provider ? (
        <Button asChild variant={passwordAllowed ? "outline" : "default"} className="mb-4 w-full">
          <a href={oidcStartUrl(redirect)}>Log in with {provider.name}</a>
        </Button>
      ) : null}
      {passwordAllowed ? (
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
      ) : desktop ? (
        <Alert>
          This server only allows logging in through the identity provider, which the desktop app
          does not support yet. Use Saldo in the browser.
        </Alert>
      ) : null}
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
