import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { SuggestInput } from "./SuggestInput";

function Harness({ onSubmit }: { onSubmit: () => void }) {
  const [value, setValue] = useState("");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit();
      }}
    >
      <SuggestInput
        aria-label="Description"
        value={value}
        onChange={setValue}
        suggestions={value ? ["Groceries", "Gas"] : []}
      />
    </form>
  );
}

describe("SuggestInput", () => {
  it("keeps free text and shows suggestions while typing", async () => {
    const user = userEvent.setup();
    render(<Harness onSubmit={vi.fn()} />);
    const field = screen.getByLabelText("Description");
    await user.type(field, "Anything");
    expect(field).toHaveValue("Anything");
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Groceries", "Gas"]);
  });

  it("takes a suggestion with the arrows and Enter without submitting", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<Harness onSubmit={onSubmit} />);
    const field = screen.getByLabelText("Description");
    await user.type(field, "G");
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(field).toHaveValue("Gas");
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("submits on Enter when nothing is highlighted, and hides on Escape", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<Harness onSubmit={onSubmit} />);
    const field = screen.getByLabelText("Description");
    await user.type(field, "G");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(onSubmit).toHaveBeenCalledOnce();
  });

  it("takes a clicked suggestion", async () => {
    const user = userEvent.setup();
    render(<Harness onSubmit={vi.fn()} />);
    await user.type(screen.getByLabelText("Description"), "G");
    await user.click(screen.getByRole("option", { name: "Groceries" }));
    expect(screen.getByLabelText("Description")).toHaveValue("Groceries");
  });
});
