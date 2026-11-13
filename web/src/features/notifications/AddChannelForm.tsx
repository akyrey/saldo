import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, call } from "@/api/client";
import { errorMessage, fieldErrors } from "@/api/errors";
import { Field } from "@/components/Field";
import { errorProps } from "@/components/field-props";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { type ChannelForm, channelBody, emptyChannelForm, type TypedKind } from "./channel-form";

const KINDS: { value: TypedKind; label: string }[] = [
  { value: "ntfy", label: "ntfy" },
  { value: "gotify", label: "Gotify" },
  { value: "email", label: "Email" },
];

/** Which form fields each kind shows, with the server's error path. */
const FIELDS: Record<TypedKind, { key: keyof ChannelForm; label: string; type?: string }[]> = {
  ntfy: [
    { key: "server_url", label: "Server URL" },
    { key: "topic", label: "Topic" },
    { key: "token", label: "Access token (optional)", type: "password" },
  ],
  gotify: [
    { key: "server_url", label: "Server URL" },
    { key: "token", label: "Application token", type: "password" },
  ],
  email: [{ key: "to", label: "Send to", type: "email" }],
};

export function AddChannelForm() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<ChannelForm>(emptyChannelForm());
  const mutation = useMutation({
    mutationFn: () => call(api.POST("/api/v1/notification-channels", { body: channelBody(form) })),
    onSuccess: async () => {
      setForm(emptyChannelForm(form.kind));
      await queryClient.invalidateQueries({ queryKey: ["notification-channels"] });
    },
  });
  const errors = fieldErrors(mutation.error);
  const set = (key: keyof ChannelForm, value: string) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  return (
    <form
      aria-label="Add a channel"
      className="grid gap-4 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate();
      }}
    >
      <Field id="kind" label="Kind" error={errors.kind}>
        <NativeSelect
          id="kind"
          value={form.kind}
          onChange={(e) => {
            mutation.reset();
            setForm({ ...emptyChannelForm(e.target.value as TypedKind), name: form.name });
          }}
        >
          {KINDS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id="name" label="Name" error={errors.name}>
        <Input
          id="name"
          value={form.name}
          onChange={(e) => set("name", e.target.value)}
          {...errorProps("name", errors.name)}
        />
      </Field>
      {FIELDS[form.kind].map(({ key, label, type }) => {
        const error = errors[`config.${key}`];
        return (
          <Field key={key} id={key} label={label} error={error}>
            <Input
              id={key}
              type={type ?? "text"}
              autoComplete="off"
              value={form[key]}
              onChange={(e) => set(key, e.target.value)}
              {...errorProps(key, error)}
            />
          </Field>
        );
      })}
      {mutation.error && Object.keys(errors).length === 0 ? (
        <Alert className="sm:col-span-2">{errorMessage(mutation.error)}</Alert>
      ) : null}
      {mutation.error && errors.config ? (
        <Alert className="sm:col-span-2">{errors.config}</Alert>
      ) : null}
      <div className="sm:col-span-2">
        <Button type="submit" disabled={mutation.isPending}>
          Add channel
        </Button>
      </div>
    </form>
  );
}
