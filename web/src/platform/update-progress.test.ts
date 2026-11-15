import { describe, expect, it, vi } from "vitest";
import { trackDownload } from "./update-progress";

describe("trackDownload", () => {
  it("reports the share downloaded when the size is known", () => {
    const report = vi.fn();
    const onEvent = trackDownload(report);

    onEvent({ event: "Started", data: { contentLength: 200 } });
    onEvent({ event: "Progress", data: { chunkLength: 50 } });
    onEvent({ event: "Progress", data: { chunkLength: 100 } });
    onEvent({ event: "Finished" });

    expect(report.mock.calls).toEqual([[0], [0.25], [0.75], [1]]);
  });

  it("reports null while the size is unknown", () => {
    const report = vi.fn();
    const onEvent = trackDownload(report);

    onEvent({ event: "Started", data: {} });
    onEvent({ event: "Progress", data: { chunkLength: 50 } });

    expect(report.mock.calls).toEqual([[null], [null]]);
  });

  it("never reports more than all of it", () => {
    const report = vi.fn();
    const onEvent = trackDownload(report);

    onEvent({ event: "Started", data: { contentLength: 10 } });
    onEvent({ event: "Progress", data: { chunkLength: 25 } });

    expect(report).toHaveBeenLastCalledWith(1);
  });
});
