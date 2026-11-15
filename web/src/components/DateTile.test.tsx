import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DateTile } from "./DateTile";

describe("DateTile", () => {
  it("renders day and weekday with the full date as its label", () => {
    render(<DateTile date="2026-11-15" today="2026-11-15" />);
    const tile = screen.getByRole("time");
    expect(tile).toHaveAttribute("datetime", "2026-11-15");
    expect(tile.getAttribute("aria-label")).toMatch(/2026/);
    expect(tile).toHaveTextContent("15");
    expect(tile.textContent).not.toMatch(/’26/);
  });

  it("adds the year only for another year", () => {
    render(<DateTile date="2025-03-02" today="2026-11-15" />);
    expect(screen.getByRole("time")).toHaveTextContent("’25");
  });

  it("falls back to the raw text for an invalid date", () => {
    render(<DateTile date="soon" today="2026-11-15" />);
    expect(screen.getByText("soon")).toBeInTheDocument();
  });
});
