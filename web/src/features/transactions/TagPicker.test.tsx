import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeApi } from "@/test/fake-api";
import { TagPicker } from "./TagPicker";

afterEach(() => {
  vi.unstubAllGlobals();
});

const tag = (id: string, name: string) => ({ id, name, created_at: "", updated_at: "" });
const TAGS = [tag("1", "groceries"), tag("2", "holiday"), tag("3", "car")];

function Harness({ initial = [] as string[] }) {
  const [value, setValue] = useState(initial);
  return (
    <>
      <TagPicker id="tags" tags={TAGS} value={value} onChange={setValue} />
      <output data-testid="value">{value.join(",")}</output>
    </>
  );
}

function renderPicker(initial: string[] = []) {
  const queryClient = new QueryClient();
  render(
    <QueryClientProvider client={queryClient}>
      <Harness initial={initial} />
    </QueryClientProvider>,
  );
  return queryClient;
}

const value = () => screen.getByTestId("value").textContent;

describe("TagPicker", () => {
  it("shows only chosen tags until the person types", () => {
    renderPicker(["2"]);
    expect(screen.getByRole("button", { name: "Remove tag holiday" })).toBeInTheDocument();
    expect(screen.queryByRole("option")).toBeNull();
    expect(screen.queryByText("groceries")).toBeNull();
  });

  it("suggests matching tags and adds one with the keyboard", async () => {
    renderPicker();
    const input = screen.getByRole("combobox", { name: "Add tag" });
    await userEvent.type(input, "ro");
    const options = within(screen.getByRole("listbox")).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["groceries", "Create tag “ro”"]);
    await userEvent.keyboard("{Enter}");
    expect(value()).toBe("1");
    expect(input).toHaveValue("");
  });

  it("removes chips by button and by Backspace in the empty field", async () => {
    renderPicker(["1", "2"]);
    await userEvent.click(screen.getByRole("button", { name: "Remove tag groceries" }));
    expect(value()).toBe("2");
    await userEvent.click(screen.getByRole("combobox", { name: "Add tag" }));
    await userEvent.keyboard("{Backspace}");
    expect(value()).toBe("");
  });

  it("creates a new tag and adds it", async () => {
    const seen = fakeApi({
      "POST /api/v1/tags": () => Response.json({ data: tag("9", "carpool") }, { status: 201 }),
    });
    const queryClient = renderPicker();
    queryClient.setQueryData(["tags"], TAGS);
    await userEvent.type(screen.getByRole("combobox", { name: "Add tag" }), "carpool");
    await userEvent.click(screen.getByRole("option", { name: "Create tag “carpool”" }));
    await waitFor(() => expect(value()).toBe("9"));
    const post = seen.find((r) => r.method === "POST");
    expect(await post?.json()).toEqual({ name: "carpool" });
    expect(queryClient.getQueryData<{ id: string }[]>(["tags"])?.map((t) => t.id)).toContain("9");
  });
});
