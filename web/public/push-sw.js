// Web Push handlers, loaded into the generated service worker through
// Workbox `importScripts` (vite.config.ts). The server sends
// `{ title, body, url, tag }` (crates/server/src/notify/webpush.rs).

self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data ? event.data.json() : {};
  } catch {
    data = { body: event.data ? event.data.text() : "" };
  }
  const title = data.title || "Saldo";
  event.waitUntil(
    self.registration.showNotification(title, {
      body: data.body || "",
      tag: data.tag || undefined,
      icon: "/pwa-192.png",
      badge: "/pwa-192.png",
      data: { url: typeof data.url === "string" && data.url.startsWith("/") ? data.url : "/" },
    }),
  );
});

// Focus an open Saldo tab and show the page, or open a new one.
self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = new URL(event.notification.data?.url || "/", self.location.origin).href;
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((windows) => {
      for (const client of windows) {
        if (new URL(client.url).origin === self.location.origin && "focus" in client) {
          return client
            .focus()
            .then((focused) =>
              focused && "navigate" in focused ? focused.navigate(url) : focused,
            );
        }
      }
      return self.clients.openWindow(url);
    }),
  );
});
