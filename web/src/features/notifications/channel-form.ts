// The "add a channel" form for ntfy, Gotify and email. Web Push channels are
// created by the browser subscription flow (push.ts), not typed in.
import type { Channel, CreateChannel } from "./queries";

export type TypedKind = "ntfy" | "gotify" | "email";

export interface ChannelForm {
  kind: TypedKind;
  name: string;
  server_url: string;
  topic: string;
  token: string;
  to: string;
}

export function emptyChannelForm(kind: TypedKind = "ntfy"): ChannelForm {
  const server_url = kind === "ntfy" ? "https://ntfy.sh" : "";
  return { kind, name: "", server_url, topic: "", token: "", to: "" };
}

/** The request body; only the fields the kind uses are sent. */
export function channelBody(form: ChannelForm): CreateChannel {
  const name = form.name.trim() || defaultName(form.kind);
  const token = form.token.trim();
  switch (form.kind) {
    case "ntfy":
      return {
        kind: "ntfy",
        name,
        config: {
          server_url: form.server_url.trim(),
          topic: form.topic.trim(),
          ...(token ? { token } : {}),
        },
      };
    case "gotify":
      return { kind: "gotify", name, config: { server_url: form.server_url.trim(), token } };
    case "email":
      return { kind: "email", name, config: { to: form.to.trim() } };
  }
}

function defaultName(kind: TypedKind): string {
  return { ntfy: "ntfy", gotify: "Gotify", email: "Email" }[kind];
}

/** One line describing where a channel delivers (never a secret). */
export function describeChannel(channel: Channel): string {
  const s = channel.settings;
  switch (channel.kind) {
    case "ntfy":
      return `${s.server_url ?? ""}/${s.topic ?? ""}${s.has_token ? " (with token)" : ""}`;
    case "gotify":
      return s.server_url ?? "";
    case "email":
      return s.to ?? "";
    case "webpush":
      return `Browser push via ${s.push_service ?? "unknown service"}`;
    default:
      return channel.kind;
  }
}
