import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Navigate, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AuthCard } from "./AuthCard";
import { meQuery, setupStatusQuery } from "./queries";

const EMPTY = {
  email: "",
  password: "",
  display_name: "",
  household_name: "",
  primary_currency: "EUR",
};

/** First run: creates the owner and the household, then logs in. */
export function SetupPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const status = useQuery(setupStatusQuery);
  const [form, setForm] = useState(EMPTY);

  const setup = useMutation({
    mutationFn: async () => {
      await call(api.POST("/api/v1/setup", { body: form }));
      await call(
        api.POST("/api/v1/auth/login", { body: { email: form.email, password: form.password } }),
      );
    },
    onSuccess: async () => {
      queryClient.setQueryData(setupStatusQuery.queryKey, false);
      await queryClient.fetchQuery(meQuery);
      await navigate({ to: "/accounts" });
    },
  });

  if (status.data === false && !setup.isPending && !setup.isSuccess) {
    return <Navigate to="/login" search={{ redirect: undefined }} />;
  }
  const errors = fieldErrors(setup.error);
  const update = (key: keyof typeof EMPTY) => (event: { target: { value: string } }) =>
    setForm((prev) => ({ ...prev, [key]: event.target.value }));

  return (
    <AuthCard title="Set up Saldo" description="Create the first account and your household.">
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          setup.mutate();
        }}
      >
        <Field id="display_name" label="Your name" error={errors.display_name}>
          <Input
            id="display_name"
            required
            value={form.display_name}
            onChange={update("display_name")}
            {...errorProps("display_name", errors.display_name)}
          />
        </Field>
        <Field id="email" label="Email" error={errors.email}>
          <Input
            id="email"
            type="email"
            autoComplete="username"
            required
            value={form.email}
            onChange={update("email")}
            {...errorProps("email", errors.email)}
          />
        </Field>
        <Field id="password" label="Password (at least 8 characters)" error={errors.password}>
          <Input
            id="password"
            type="password"
            autoComplete="new-password"
            required
            minLength={8}
            value={form.password}
            onChange={update("password")}
            {...errorProps("password", errors.password)}
          />
        </Field>
        <Field id="household_name" label="Household name" error={errors.household_name}>
          <Input
            id="household_name"
            required
            value={form.household_name}
            onChange={update("household_name")}
            {...errorProps("household_name", errors.household_name)}
          />
        </Field>
        <Field
          id="primary_currency"
          label="Main currency (ISO code)"
          error={errors.primary_currency}
        >
          <Input
            id="primary_currency"
            required
            maxLength={3}
            pattern="[A-Za-z]{3}"
            title="A three-letter ISO code such as EUR"
            value={form.primary_currency}
            onChange={(e) =>
              setForm((prev) => ({ ...prev, primary_currency: e.target.value.toUpperCase() }))
            }
            {...errorProps("primary_currency", errors.primary_currency)}
          />
        </Field>
        {setup.error && Object.keys(errors).length === 0 ? (
          <Alert>{errorMessage(setup.error)}</Alert>
        ) : null}
        <Button type="submit" disabled={setup.isPending}>
          {setup.isPending ? "Setting up…" : "Create household"}
        </Button>
      </form>
    </AuthCard>
  );
}
