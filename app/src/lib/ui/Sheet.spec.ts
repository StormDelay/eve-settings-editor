// Component test: vitest + jsdom.
import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import Sheet from "./Sheet.svelte";
import { createRawSnippet } from "svelte";
import "$lib/test/setup";

// Two focusables, so the trap has something to cycle between.
const body = createRawSnippet(() => ({
  render: () => `<div><button>First</button><button>Last</button></div>`,
}));

describe("Sheet", () => {
  // The `.modal` this replaces had none of this: no Escape, no trap, no focus
  // restoration. That is an accessibility floor rather than a feature.
  test("is a modal dialog named by its title", () => {
    render(Sheet, { title: "Insert entry", onclose: () => {}, children: body });

    const dialog = screen.getByRole("dialog", { name: "Insert entry" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
  });

  test("Escape closes it", async () => {
    const onclose = vi.fn();
    render(Sheet, { title: "Insert entry", onclose, children: body });

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(onclose).toHaveBeenCalledTimes(1);
  });

  test("a backdrop click closes it, a click inside does not", async () => {
    const onclose = vi.fn();
    const { container } = render(Sheet, { title: "Insert entry", onclose, children: body });

    await fireEvent.click(screen.getByRole("dialog"));
    expect(onclose).not.toHaveBeenCalled();

    await fireEvent.click(container.querySelector(".overlay")!);
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  test("focus moves into the sheet on open", () => {
    render(Sheet, { title: "Insert entry", onclose: () => {}, children: body });

    expect(document.activeElement?.textContent).toBe("First");
  });

  // Without this a keyboard user lands back at the top of the document every
  // time a dialog closes, and has to walk the whole page again.
  test("focus returns to whatever opened it", async () => {
    const opener = document.createElement("button");
    opener.textContent = "Open";
    document.body.appendChild(opener);
    opener.focus();
    expect(document.activeElement).toBe(opener);

    const { unmount } = render(Sheet, { title: "Insert entry", onclose: () => {}, children: body });
    expect(document.activeElement).not.toBe(opener);

    unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  test("Tab wraps from the last focusable back to the first", async () => {
    render(Sheet, { title: "Insert entry", onclose: () => {}, children: body });

    const last = screen.getByRole("button", { name: "Last" });
    last.focus();
    await fireEvent.keyDown(window, { key: "Tab" });

    expect(document.activeElement?.textContent).toBe("First");
  });

  test("Shift+Tab wraps from the first back to the last", async () => {
    render(Sheet, { title: "Insert entry", onclose: () => {}, children: body });

    screen.getByRole("button", { name: "First" }).focus();
    await fireEvent.keyDown(window, { key: "Tab", shiftKey: true });

    expect(document.activeElement?.textContent).toBe("Last");
  });

  test("closed renders nothing and ignores Escape", async () => {
    const onclose = vi.fn();
    render(Sheet, { title: "Insert entry", open: false, onclose, children: body });

    expect(screen.queryByRole("dialog")).toBeNull();

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onclose).not.toHaveBeenCalled();
  });
});
