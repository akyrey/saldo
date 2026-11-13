import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { checkServer, normalizeServerUrl } from "@/api/server-check";
import { Field } from "@/components/Field";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { platform } from "@/platform";
import { AuthCard } from "./AuthCard";

/** Desktop app only: which Saldo server to use. Checked before it is kept. */
export function ConnectPage() {
  const navigate = useNavigate();
  const [address, setAddress] = useState(platform().serverUrl());

  const connect = useMutation({
    mutationFn: async () => {
      const url = normalizeServerUrl(address);
      if (!url) throw new Error("Enter an address like https://money.example.com.");
      const result = await checkServer(url);
      if (!result.ok) throw new Error(result.reason);
      return url;
    },
    onSuccess: async (url) => {
      platform().setServerUrl(url);
      // A new server means a new account: any old token is meaningless there.
      await platform().clearToken();
      await navigate({ to: "/login", search: { redirect: undefined } });
    },
  });

  return (
    <AuthCard title="Connect to Saldo" description="The address of your Saldo server.">
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          connect.mutate();
        }}
      >
        <Field id="server" label="Server address">
          <Input
            id="server"
            inputMode="url"
            autoComplete="url"
            placeholder="https://money.example.com"
            required
            value={address}
            onChange={(e) => setAddress(e.target.value)}
          />
        </Field>
        {connect.error ? <Alert>{connect.error.message}</Alert> : null}
        <Button type="submit" disabled={connect.isPending}>
          {connect.isPending ? "Checking…" : "Connect"}
        </Button>
      </form>
    </AuthCard>
  );
}
