import { describe, expect, it } from "vitest";
import { channelBody, describeChannel, emptyChannelForm } from "./channel-form";
import type { Channel } from "./queries";

describe("channelBody", () => {
  it("sends ntfy settings and leaves out an empty token", () => {
    const form = { ...emptyChannelForm("ntfy"), topic: " bills " };
    expect(channelBody(form)).toEqual({
      kind: "ntfy",
      name: "ntfy",
      config: { server_url: "https://ntfy.sh", topic: "bills" },
    });
    expect(channelBody({ ...form, token: "tk", name: "Phone" }).config).toEqual({
      server_url: "https://ntfy.sh",
      topic: "bills",
      token: "tk",
    });
  });

  it("sends only the fields Gotify and email use", () => {
    const gotify = {
      ...emptyChannelForm("gotify"),
      server_url: "http://g.lan",
      token: "t",
      to: "x",
    };
    expect(channelBody(gotify)).toEqual({
      kind: "gotify",
      name: "Gotify",
      config: { server_url: "http://g.lan", token: "t" },
    });
    const email = { ...emptyChannelForm("email"), to: " me@example.com ", name: "Mail" };
    expect(channelBody(email)).toEqual({
      kind: "email",
      name: "Mail",
      config: { to: "me@example.com" },
    });
  });
});

describe("describeChannel", () => {
  const base: Channel = {
    id: "c1",
    user_id: "u1",
    kind: "ntfy",
    name: "Phone",
    active: true,
    settings: { server_url: "https://ntfy.sh", topic: "bills", has_token: true },
    event_types: ["bill.due_soon", "bill.overdue"],
    created_at: "",
    updated_at: "",
  };

  it("names where each kind delivers", () => {
    expect(describeChannel(base)).toBe("https://ntfy.sh/bills (with token)");
    expect(describeChannel({ ...base, kind: "gotify" })).toBe("https://ntfy.sh");
    expect(
      describeChannel({ ...base, kind: "email", settings: { to: "a@b.c", has_token: false } }),
    ).toBe("a@b.c");
    expect(
      describeChannel({
        ...base,
        kind: "webpush",
        settings: { push_service: "https://fcm.googleapis.com", has_token: false },
      }),
    ).toBe("Browser push via https://fcm.googleapis.com");
    expect(describeChannel({ ...base, kind: "other" })).toBe("other");
  });
});
