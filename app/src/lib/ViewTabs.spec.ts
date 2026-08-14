// The view strip. Fault (c) was that it changed membership and width as files
// loaded and pairings landed — every tab behind its own `{#if}`, the strip
// behind a seventh, and with nothing qualifying it vanished entirely.
//
// `page.spec.ts` pins the property through the whole shell (identical before and
// after an open). These are the unit-level cases: what each disabled tab SAYS,
// and that the conditions did not change while the presentation did.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ViewTabs from "$lib/ViewTabs.svelte";
import { subject } from "$lib/subject.svelte";
import type { OpenOutcome } from "$lib/api";
import type { View } from "$lib/views";

const opened = (file_name: string): OpenOutcome => ({
  status: "opened",
  path: `/eve/${file_name}`,
  file_name,
  fidelity: { state: "editable" },
  tree: {
    label: "root", kind: "dict", display: "{}", path: [],
    editable: false, edit_text: null, removable: false, in_shared: false, children: [],
  },
});

const mount = (value: View = "raw") => render(ViewTabs, { value });
const tab = (name: string) => screen.getByRole("tab", { name });
const names = () => screen.getAllByRole("tab").map((t) => t.textContent?.trim());

describe("with nothing open", () => {
  test("all six render, five disabled, each with a reason", () => {
    mount();
    expect(names()).toEqual(["Layout", "Overview", "Autofill", "Keybinds", "Probes", "Raw"]);
    for (const v of ["Layout", "Overview", "Autofill", "Keybinds", "Probes"]) {
      expect(tab(v).getAttribute("aria-disabled")).toBe("true");
      expect(tab(v).getAttribute("title")).toBeTruthy();
    }
    // Raw is the escape hatch and is always reachable — which is what makes the
    // post-open fallback always resolve.
    expect(tab("Raw").getAttribute("aria-disabled")).toBeNull();
  });

  test("Layout's reason says to open a character, not that the file lacks a layout", () => {
    mount();
    expect(tab("Layout").getAttribute("title")).toMatch(/open a character/i);
  });

  test("a disabled tab does not become the value when clicked", async () => {
    mount("raw");
    await fireEvent.click(tab("Overview"));
    expect(tab("Overview").getAttribute("aria-selected")).toBe("false");
    expect(tab("Raw").getAttribute("aria-selected")).toBe("true");
  });
});

describe("with a character open", () => {
  test("the four account-scoped tabs unlock and Layout still explains itself", () => {
    subject.slots.char = opened("core_char_950.dat");
    mount();
    for (const v of ["Overview", "Autofill", "Keybinds", "Probes"]) {
      expect(tab(v).getAttribute("aria-disabled")).toBeNull();
    }
    expect(tab("Layout").getAttribute("aria-disabled")).toBe("true");
    expect(tab("Layout").getAttribute("title")).toMatch(/no saved window layout/i);
  });

  test("Layout unlocks once the document is known to have windows", () => {
    subject.slots.char = opened("core_char_950.dat");
    subject.layoutAvailable = true;
    mount();
    expect(tab("Layout").getAttribute("aria-disabled")).toBeNull();
  });

  // §5.10's one free line — the shell hands `onpick` a `mainView = "file"`, so a
  // tab click is also the way out of the Accounts takeover. It must fire on a
  // USER pick only.
  test("picking a tab reports it", async () => {
    subject.slots.char = opened("core_char_950.dat");
    let picked: string | null = null;
    render(ViewTabs, { value: "raw", onpick: () => (picked = "yes") });
    await fireEvent.click(tab("Overview"));
    expect(picked).toBe("yes");
  });
});
