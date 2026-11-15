import { describe, expect, it } from "vitest";
import {
  addDays,
  dateParts,
  dayHeading,
  dayName,
  isAfterDay,
  parseIsoDate,
  todayIso,
} from "./dates";

describe("todayIso", () => {
  it("formats the local date with padding", () => {
    expect(todayIso(new Date(2026, 0, 5, 23, 59))).toBe("2026-01-05");
  });
});

describe("parseIsoDate", () => {
  it("reads a calendar date as local midnight", () => {
    expect(parseIsoDate("2026-11-15")).toEqual(new Date(2026, 10, 15));
  });

  it("returns null for text that is not a date", () => {
    expect(parseIsoDate("15/11/2026")).toBeNull();
    expect(parseIsoDate("2026-02-30")).toBeNull();
    expect(parseIsoDate("")).toBeNull();
  });
});

describe("isAfterDay", () => {
  it("compares calendar dates", () => {
    expect(isAfterDay("2026-11-16", "2026-11-15")).toBe(true);
    expect(isAfterDay("2026-11-15", "2026-11-15")).toBe(false);
    expect(isAfterDay("2025-12-31", "2026-01-01")).toBe(false);
  });
});

describe("dateParts", () => {
  it("splits a date into weekday, day, month and a long label", () => {
    expect(dateParts("2026-11-15", "en-GB")).toEqual({
      weekday: "Sun",
      day: "15",
      month: "Nov",
      year: "2026",
      label: "Sunday, 15 November 2026",
    });
  });

  it("returns null for an invalid date", () => {
    expect(dateParts("nope", "en-GB")).toBeNull();
  });
});

describe("addDays", () => {
  it("moves across month and year ends", () => {
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
  });

  it("leaves invalid input alone", () => {
    expect(addDays("x", 1)).toBe("x");
  });
});

describe("dayHeading", () => {
  const today = "2026-11-15";

  it("names today, yesterday and tomorrow", () => {
    expect(dayHeading("2026-11-15", today, "en-GB")).toBe("Today");
    expect(dayHeading("2026-11-14", today, "en-GB")).toBe("Yesterday");
    expect(dayHeading("2026-11-16", today, "en-GB")).toBe("Tomorrow");
  });

  it("shows weekday, day and month, and the year only for other years", () => {
    expect(dayHeading("2026-11-07", today, "en-GB")).toBe("Sat 7 Nov");
    expect(dayHeading("2025-11-07", today, "en-GB")).toBe("Fri, 7 Nov 2025");
  });
});

describe("dayName", () => {
  it("is relative near today and the full weekday otherwise", () => {
    expect(dayName("2026-11-14", "2026-11-15", "en-GB")).toBe("Yesterday");
    expect(dayName("2026-11-07", "2026-11-15", "en-GB")).toBe("Saturday");
    expect(dayName("bad", "2026-11-15", "en-GB")).toBe("bad");
  });
});
