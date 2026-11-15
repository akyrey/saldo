import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KindBadge } from "./KindBadge";

describe("KindBadge", () => {
  it("colours each known kind with its own token", () => {
    render(
      <>
        <KindBadge kind="withdrawal" />
        <KindBadge kind="deposit" />
        <KindBadge kind="transfer" />
      </>,
    );
    expect(screen.getByText("Withdrawal")).toHaveClass("text-kind-withdrawal");
    expect(screen.getByText("Deposit")).toHaveClass("text-kind-deposit");
    expect(screen.getByText("Transfer")).toHaveClass("text-kind-transfer");
  });

  it("shows an unknown kind as plain text without colour", () => {
    render(<KindBadge kind="mystery" />);
    const badge = screen.getByText("mystery");
    expect(badge.className).not.toMatch(/text-kind-/);
  });
});
