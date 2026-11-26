import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setPlatformForTests, type Platform } from "@/platform";
import Markdown from "./Markdown";

afterEach(() => setPlatformForTests(null));

describe("Markdown", () => {
  it("renders emphasis, lists and GitHub-style tables", () => {
    render(
      <Markdown text={"**paid** in cash\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |"} />,
    );
    expect(screen.getByText("paid").tagName).toBe("STRONG");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByRole("table")).toBeInTheDocument();
  });

  it("keeps a single line break as a break", () => {
    const { container } = render(<Markdown text={"first\nsecond"} />);
    expect(container.querySelector("br")).not.toBeNull();
  });

  it("shows raw HTML as text instead of running it", () => {
    const { container } = render(<Markdown text={'<img src=x onerror="alert(1)">'} />);
    expect(container.querySelector("img")).toBeNull();
  });

  it("drops javascript: links", () => {
    render(<Markdown text={"[click](javascript:alert(1))"} />);
    expect(screen.getByText("click").closest("a")?.getAttribute("href") ?? "").not.toContain(
      "javascript",
    );
  });

  it("opens links through the platform, outside the app", async () => {
    const openExternal = vi.fn(async () => {});
    setPlatformForTests({ openExternal } as unknown as Platform);
    render(<Markdown text={"see https://example.com/receipt"} />);
    await userEvent.click(screen.getByRole("link", { name: "https://example.com/receipt" }));
    expect(openExternal).toHaveBeenCalledWith("https://example.com/receipt");
  });
});
