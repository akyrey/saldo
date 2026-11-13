import { describe, expect, it, vi } from "vitest";
import { fetchAllPages, pageCount } from "./pagination";

describe("fetchAllPages", () => {
  it("keeps fetching until it has the total", async () => {
    const fetchPage = vi.fn((page: number) =>
      Promise.resolve({
        data: page < 3 ? [page * 10, page * 10 + 1] : [30],
        meta: { page, per_page: 2, total: 5 },
      }),
    );

    expect(await fetchAllPages(fetchPage)).toEqual([10, 11, 20, 21, 30]);
    expect(fetchPage).toHaveBeenCalledTimes(3);
  });

  it("stops on an empty page", async () => {
    const fetchPage = () =>
      Promise.resolve({ data: [], meta: { page: 1, per_page: 50, total: 9 } });
    expect(await fetchAllPages(fetchPage)).toEqual([]);
  });
});

describe("pageCount", () => {
  it("rounds up and is at least one", () => {
    expect(pageCount({ page: 1, per_page: 50, total: 0 })).toBe(1);
    expect(pageCount({ page: 1, per_page: 50, total: 101 })).toBe(3);
  });
});
