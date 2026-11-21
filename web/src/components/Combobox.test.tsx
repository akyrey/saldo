import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Combobox } from "./Combobox";

const OPTIONS = [
  { value: "g", label: "Groceries" },
  { value: "c", label: "Car insurance" },
  { value: "h", label: "Holidays" },
];

function Harness({ initial = "", clearable = false }: { initial?: string; clearable?: boolean }) {
  const [value, setValue] = useState(initial);
  return (
    <>
      <label htmlFor="category">Category</label>
      <Combobox
        id="category"
        label="Category"
        options={OPTIONS}
        value={value}
        onChange={setValue}
        clearable={clearable}
      />
      <output data-testid="value">{value}</output>
    </>
  );
}

const field = () => screen.getByLabelText("Category");
const value = () => screen.getByTestId("value").textContent;

describe("Combobox", () => {
  it("shows the chosen option's label", () => {
    render(<Harness initial="c" />);
    expect(field()).toHaveValue("Car insurance");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("filters while typing and picks with the keyboard", async () => {
    render(<Harness />);
    await userEvent.type(field(), "ho");
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Holidays"]);
    await userEvent.keyboard("{Enter}");
    expect(value()).toBe("h");
    expect(field()).toHaveValue("Holidays");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("lists everything on focus and moves with the arrow keys", async () => {
    render(<Harness />);
    await userEvent.click(field());
    expect(screen.getAllByRole("option")).toHaveLength(3);
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(value()).toBe("h");
  });

  it("picks with the mouse", async () => {
    render(<Harness />);
    await userEvent.type(field(), "car");
    await userEvent.click(screen.getByRole("option", { name: "Car insurance" }));
    expect(value()).toBe("c");
  });

  it("keeps the old choice after Escape or leaving the field", async () => {
    render(<Harness initial="g" />);
    await userEvent.clear(field());
    await userEvent.type(field(), "hol");
    await userEvent.keyboard("{Escape}");
    expect(field()).toHaveValue("Groceries");
    await userEvent.type(field(), "x");
    await userEvent.tab();
    expect(field()).toHaveValue("Groceries");
    expect(value()).toBe("g");
  });

  it("says when nothing matches", async () => {
    render(<Harness />);
    await userEvent.type(field(), "zzz");
    expect(screen.getByText("No matches")).toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    expect(value()).toBe("");
  });

  it("can be cleared when the field is optional", async () => {
    render(<Harness initial="g" clearable />);
    await userEvent.click(screen.getByRole("button", { name: "Clear Category" }));
    expect(value()).toBe("");
    expect(field()).toHaveValue("");
  });

  describe("with a create option", () => {
    function CreateHarness({ onCreate }: { onCreate: (text: string) => void }) {
      return (
        <>
          <label htmlFor="category">Category</label>
          <Combobox
            id="category"
            options={OPTIONS}
            value=""
            onChange={() => undefined}
            create={{
              // Offered only for a name no option has yet.
              label: (text) => (text === "Holidays" ? null : `Create “${text}”`),
              onCreate,
            }}
          />
        </>
      );
    }

    it("offers to create what was typed after the matches", async () => {
      render(<CreateHarness onCreate={vi.fn()} />);
      await userEvent.type(field(), "Car");
      expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
        "Car insurance",
        "Create “Car”",
      ]);
    });

    it("creates with the keyboard and with the mouse", async () => {
      const onCreate = vi.fn();
      render(<CreateHarness onCreate={onCreate} />);
      await userEvent.type(field(), "Gym");
      expect(screen.queryByText("No matches")).toBeNull();
      await userEvent.keyboard("{Enter}");
      expect(onCreate).toHaveBeenLastCalledWith("Gym");
      await userEvent.type(field(), "Car");
      await userEvent.click(screen.getByRole("option", { name: "Create “Car”" }));
      expect(onCreate).toHaveBeenLastCalledWith("Car");
    });

    it("does not offer to create an empty name or one the label refuses", async () => {
      render(<CreateHarness onCreate={vi.fn()} />);
      await userEvent.click(field());
      expect(screen.queryByText(/Create/)).toBeNull();
      await userEvent.type(field(), "Holidays");
      expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Holidays"]);
    });
  });
});
