import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { NotesField } from "./NotesField";

function Harness({ initial = "" }: { initial?: string }) {
  const [value, setValue] = useState(initial);
  return <NotesField id="notes" aria-label="Notes" value={value} onChange={setValue} />;
}

describe("NotesField", () => {
  it("accepts several lines", async () => {
    render(<Harness />);
    const field = screen.getByLabelText("Notes");
    await userEvent.type(field, "first{Enter}second");
    expect(field).toHaveValue("first\nsecond");
  });

  it("previews the notes as Markdown and goes back to editing", async () => {
    render(<Harness initial="**bold**" />);
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));
    expect((await screen.findByText("bold")).tagName).toBe("STRONG");
    expect(screen.queryByLabelText("Notes")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.getByLabelText("Notes")).toHaveValue("**bold**");
  });

  it("offers no preview while the notes are empty", () => {
    render(<Harness />);
    expect(screen.getByRole("button", { name: "Preview" })).toBeDisabled();
  });
});
