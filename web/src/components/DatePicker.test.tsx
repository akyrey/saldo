import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { addDays, todayIso } from "@/lib/dates";
import { DatePicker } from "./DatePicker";

function Harness({ initial = "", required = false, spy = vi.fn() }) {
  const [value, setValue] = useState(initial);
  return (
    <>
      <label htmlFor="d">Date</label>
      <DatePicker
        id="d"
        value={value}
        required={required}
        onChange={(v) => {
          spy(v);
          setValue(v);
        }}
      />
      <output data-testid="value">{value}</output>
    </>
  );
}

describe("DatePicker", () => {
  it("reports a typed date only once it is complete and real", async () => {
    const user = userEvent.setup();
    const spy = vi.fn();
    render(<Harness spy={spy} />);
    await user.type(screen.getByLabelText("Date"), "2026-02-30");
    expect(spy).not.toHaveBeenCalled();
    await user.clear(screen.getByLabelText("Date"));
    await user.type(screen.getByLabelText("Date"), "2026-02-28");
    expect(screen.getByTestId("value")).toHaveTextContent("2026-02-28");
  });

  it("goes back to the last real date when half-typed text is left", async () => {
    const user = userEvent.setup();
    render(<Harness initial="2026-11-07" />);
    const field = screen.getByLabelText("Date");
    await user.clear(field);
    await user.type(field, "2026-1");
    await user.tab();
    expect(field).toHaveValue("");
    expect(screen.getByTestId("value")).toHaveTextContent("");
  });

  it("picks a day from the calendar", async () => {
    const user = userEvent.setup();
    render(<Harness initial="2026-11-07" />);
    await user.click(screen.getByRole("button", { name: "Choose date" }));
    const grid = await screen.findByRole("grid");
    await user.click(within(grid).getByRole("button", { name: /November 12/ }));
    expect(screen.getByTestId("value")).toHaveTextContent("2026-11-12");
    expect(screen.queryByRole("grid")).not.toBeInTheDocument();
  });

  it("offers today, yesterday and, when optional, clear", async () => {
    const user = userEvent.setup();
    render(<Harness initial="2026-11-07" />);
    await user.click(screen.getByRole("button", { name: "Choose date" }));
    await user.click(await screen.findByRole("button", { name: "Yesterday" }));
    expect(screen.getByTestId("value")).toHaveTextContent(addDays(todayIso(), -1));
    await user.click(screen.getByRole("button", { name: "Choose date" }));
    await user.click(await screen.findByRole("button", { name: "Today" }));
    expect(screen.getByTestId("value")).toHaveTextContent(todayIso());
    await user.click(screen.getByRole("button", { name: "Choose date" }));
    await user.click(await screen.findByRole("button", { name: "Clear" }));
    expect(screen.getByTestId("value")).toHaveTextContent("");
  });

  it("has no clear button when a date is required", async () => {
    const user = userEvent.setup();
    render(<Harness initial="2026-11-07" required />);
    await user.click(screen.getByRole("button", { name: "Choose date" }));
    await screen.findByRole("button", { name: "Today" });
    expect(screen.queryByRole("button", { name: "Clear" })).not.toBeInTheDocument();
  });
});
