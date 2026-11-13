// Picks an option in a `Combobox` the way a person does: type, then click.
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

/** Types `typed` (default: the label) into `input` and clicks the option labelled `label`. */
export async function choose(input: HTMLElement, label: string, typed = label): Promise<void> {
  await userEvent.clear(input);
  await userEvent.type(input, typed);
  const listId = input.getAttribute("aria-controls") ?? "";
  const list = document.getElementById(listId) ?? screen.getByRole("listbox");
  const option = within(list)
    .getAllByRole("option")
    .find((o) => o.firstElementChild?.textContent === label);
  if (!option) throw new Error(`no option labelled ${label}`);
  await userEvent.click(option);
}
