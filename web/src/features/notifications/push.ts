// Web Push in the browser: ask permission, subscribe through the service
// worker with the server's VAPID key, and hand the subscription to the API.

/** Whether this browser can receive Web Push at all. */
export function pushSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

/** The VAPID key (base64url) as the bytes `pushManager.subscribe` wants. */
export function base64UrlToBytes(text: string): Uint8Array<ArrayBuffer> {
  const padded = text + "=".repeat((4 - (text.length % 4)) % 4);
  const binary = atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = new Uint8Array(new ArrayBuffer(binary.length));
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

export class PushDeniedError extends Error {
  constructor() {
    super("Notifications are blocked for this site. Allow them in the browser settings.");
  }
}

/**
 * Subscribes this browser and returns `PushSubscription.toJSON()`, the
 * config of a `webpush` channel. Reuses an existing subscription.
 */
export async function subscribeBrowser(publicKey: string): Promise<PushSubscriptionJSON> {
  const permission = await Notification.requestPermission();
  if (permission !== "granted") throw new PushDeniedError();
  const registration = await navigator.serviceWorker.ready;
  const existing = await registration.pushManager.getSubscription();
  const subscription =
    existing ??
    (await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: base64UrlToBytes(publicKey),
    }));
  return subscription.toJSON();
}
