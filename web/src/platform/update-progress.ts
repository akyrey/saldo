// Turns the updater's download events into "how much is done". Kept apart
// from desktop.ts so it can be tested without Tauri.

/** The events `Update.downloadAndInstall` reports (@tauri-apps/plugin-updater). */
export type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** Reports the share downloaded (0 to 1), or `null` when the size is unknown. */
export function trackDownload(
  report: (fraction: number | null) => void,
): (event: DownloadEvent) => void {
  let total: number | null = null;
  let received = 0;
  return (event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? null;
      report(total ? 0 : null);
    } else if (event.event === "Progress") {
      received += event.data.chunkLength;
      report(total ? Math.min(received / total, 1) : null);
    } else {
      report(1);
    }
  };
}
