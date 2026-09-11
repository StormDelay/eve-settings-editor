// Component test: vitest + jsdom.
import { describe, expect, test } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import Tabs from "./Tabs.svelte";
import "$lib/test/setup";

const TABS = [
  { id: "raw", label: "Raw" },
  { id: "overview", label: "Overview" },
  { id: "keys", label: "Keybinds" },
];

const selected = () => screen.getByRole("tab", { selected: true }).textContent;

describe("Tabs", () => {
  // None of the three strips this replaces had all of these. The main view strip
  // was a bare <span> of buttons with no roles at all.
  test("every tab is a tab, and exactly one is selected", () => {
    render(Tabs, { tabs: TABS, value: "overview", ariaLabel: "View" });

    expect(screen.getAllByRole("tab").length).toBe(3);
    expect(screen.getAllByRole("tab", { selected: true }).length).toBe(1);
    expect(selected()).toBe("Overview");
  });

  test("the tablist carries its accessible name", () => {
    render(Tabs, { tabs: TABS, value: "raw", ariaLabel: "View" });
    expect(screen.getByRole("tablist", { name: "View" })).toBeTruthy();
  });

  // A roving tabindex is what keeps the whole strip to one Tab stop.
  test("only the selected tab is in the tab order", () => {
    render(Tabs, { tabs: TABS, value: "overview", ariaLabel: "View" });

    const order = screen.getAllByRole("tab").map((t) => t.getAttribute("tabindex"));
    expect(order).toEqual(["-1", "0", "-1"]);
  });

  test("Right and Left move the selection, wrapping at both ends", async () => {
    render(Tabs, { tabs: TABS, value: "raw", ariaLabel: "View" });
    const list = screen.getByRole("tablist");

    await fireEvent.keyDown(list, { key: "ArrowRight" });
    expect(selected()).toBe("Overview");

    await fireEvent.keyDown(list, { key: "ArrowLeft" });
    expect(selected()).toBe("Raw");

    await fireEvent.keyDown(list, { key: "ArrowLeft" });
    expect(selected()).toBe("Keybinds");
  });

  test("Home and End jump to the ends", async () => {
    render(Tabs, { tabs: TABS, value: "overview", ariaLabel: "View" });
    const list = screen.getByRole("tablist");

    await fireEvent.keyDown(list, { key: "End" });
    expect(selected()).toBe("Keybinds");

    await fireEvent.keyDown(list, { key: "Home" });
    expect(selected()).toBe("Raw");
  });

  test("clicking a tab selects it", async () => {
    render(Tabs, { tabs: TABS, value: "raw", ariaLabel: "View" });

    await fireEvent.click(screen.getByRole("tab", { name: "Keybinds" }));

    expect(selected()).toBe("Keybinds");
  });

  // Disabled rather than omitted, so the strip stops rearranging under the
  // cursor as files load. Phase 2 is what starts passing this.
  test("a disabled tab is announced, explained, and cannot be selected", async () => {
    const tabs = [
      { id: "raw", label: "Raw" },
      { id: "keys", label: "Keybinds", disabled: true, disabledReason: "No file open" },
    ];
    render(Tabs, { tabs, value: "raw", ariaLabel: "View" });

    const off = screen.getByRole("tab", { name: "Keybinds" });
    expect(off.getAttribute("aria-disabled")).toBe("true");
    expect(off.getAttribute("title")).toBe("No file open");

    await fireEvent.click(off);
    expect(selected()).toBe("Raw");
  });

  test("arrow movement skips a disabled tab", async () => {
    const tabs = [
      { id: "a", label: "Alpha" },
      { id: "b", label: "Bravo", disabled: true, disabledReason: "Not yet" },
      { id: "c", label: "Charlie" },
    ];
    render(Tabs, { tabs, value: "a", ariaLabel: "View" });

    await fireEvent.keyDown(screen.getByRole("tablist"), { key: "ArrowRight" });

    expect(selected()).toBe("Charlie");
  });
});
