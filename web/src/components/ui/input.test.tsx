import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Input, Textarea } from "./input";

describe("Input and Textarea", () => {
  it("turn autocorrect and spell check off by default", () => {
    render(
      <>
        <Input aria-label="name" />
        <Textarea aria-label="notes" />
      </>,
    );
    for (const field of [screen.getByLabelText("name"), screen.getByLabelText("notes")]) {
      expect(field).toHaveAttribute("autocorrect", "off");
      expect(field).toHaveAttribute("spellcheck", "false");
    }
  });

  it("let a field turn spell check back on", () => {
    render(<Textarea aria-label="notes" spellCheck />);
    expect(screen.getByLabelText("notes")).toHaveAttribute("spellcheck", "true");
  });
});
