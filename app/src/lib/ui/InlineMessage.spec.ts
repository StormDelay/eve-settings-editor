// Component test: vitest + jsdom.
import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import InlineMessage from "./InlineMessage.svelte";
import { text } from "./snippet";
import "$lib/test/setup";

describe("InlineMessage", () => {
  // New in Phase 1. Today `.error` is a bare <p> at nine sites with no live
  // region, so a validation failure is silent to a screen reader.
  test("warn and error are assertive live regions", () => {
    const { unmount } = render(InlineMessage, { variant: "warn", children: text("Careful") });
    expect(screen.getByRole("alert").textContent).toContain("Careful");
    unmount();

    render(InlineMessage, { variant: "error", children: text("Broken") });
    expect(screen.getByRole("alert").textContent).toContain("Broken");
  });

  test("info and success are polite ones", () => {
    const { unmount } = render(InlineMessage, { children: text("Noted") });
    expect(screen.getByRole("status").textContent).toContain("Noted");
    unmount();

    render(InlineMessage, { variant: "success", children: text("Saved") });
    expect(screen.getByRole("status").textContent).toContain("Saved");
  });

  test("an explicit role wins over the variant's default", () => {
    render(InlineMessage, { variant: "error", role: "status", children: text("Quiet") });
    expect(screen.getByRole("status")).toBeTruthy();
  });

  test("dismissible renders a labelled control that calls ondismiss", async () => {
    const ondismiss = vi.fn();
    render(InlineMessage, { dismissible: true, ondismiss, children: text("Go away") });

    await fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));

    expect(ondismiss).toHaveBeenCalledTimes(1);
  });

  test("a title renders as a lead-in beside the body", () => {
    render(InlineMessage, { variant: "warn", title: "Conflict", children: text("two accounts") });
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("Conflict");
    expect(alert.textContent).toContain("two accounts");
  });
});
