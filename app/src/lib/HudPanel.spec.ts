// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// HudPanel is where a number typed into an input becomes the text the backend
// parses. Its two documented rules — an Int field rounds before writing because
// <input type="number"> does not enforce integrality and the backend's Int
// parser rejects a fractional string outright, and a blank or non-numeric input
// writes nothing at all — are invisible to a type check and were previously
// covered by nothing.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import HudPanel from "$lib/HudPanel.svelte";
import type { Hud, HudEntry, HudKind, HudScope, SetTarget } from "$lib/api";

const SET: SetTarget = { how: "set", path: [{ s: "d", i: 0 }] };
const UNAVAILABLE: SetTarget = { how: "unavailable" };

function entry(
  name: string,
  kind: HudKind,
  value: string | null,
  opts: { scope?: HudScope; set?: SetTarget; default?: string } = {},
): HudEntry {
  return {
    name,
    kind,
    value,
    default: opts.default ?? "0",
    scope: opts.scope ?? "char",
    set: opts.set ?? SET,
  };
}

/// A HUD with every field present and writable unless overridden.
function hud(...overrides: HudEntry[]): Hud {
  const base: HudEntry[] = [
    entry("ship_offset", "float", "-189"),
    entry("ship_top", "bool", "true", { scope: "account" }),
    entry("fighter_x", "int", "326"),
    entry("fighter_y", "int", "54"),
    entry("fighter_detached", "bool", "false", { scope: "account" }),
    entry("fighter_shown", "bool", "true", { scope: "account" }),
    entry("neocom_width", "int", "37", { scope: "account", default: "37" }),
    entry("badge_x", "int", "2519"),
    entry("badge_y", "int", "131"),
  ];
  const byName = new Map(base.map((e) => [e.name, e]));
  for (const o of overrides) byName.set(o.name, o);
  return { entries: [...byName.values()] };
}

function mount(h: Hud, props: Partial<Parameters<typeof render>[1]> = {}) {
  const onSet = vi.fn();
  const onSelectKind = vi.fn();
  render(HudPanel, {
    hud: h, readOnly: false, onSet, onSelectKind,
    // The neocom bar itself is optional (no character file, no bar), but its
    // four callbacks are required props — stub them so mounting a HUD-only
    // fixture doesn't need to know about neocom at all.
    onNeocomReorder: vi.fn(), onNeocomRemove: vi.fn(), onNeocomAdd: vi.fn(), onNeocomReset: vi.fn(),
    ...(props as object),
  });
  return { onSet, onSelectKind };
}

/// The panel repeats labels across groups ("x" and "y" belong to both the
/// fighter UI and the badge), so every query is scoped to its group first.
function group(title: string): HTMLElement {
  return screen.getByRole("button", { name: title }).closest(".group") as HTMLElement;
}

function row(groupTitle: string, label: string): HTMLLabelElement {
  const found = [...group(groupTitle).querySelectorAll("label")].find(
    (l) => l.querySelector(".label")?.textContent?.trim() === label,
  );
  if (!found) throw new Error(`no row "${label}" in group "${groupTitle}"`);
  return found as HTMLLabelElement;
}

function input(groupTitle: string, label: string): HTMLInputElement {
  return row(groupTitle, label).querySelector("input")!;
}

describe("writing a value", () => {
  test("an int field rounds a fractional entry before writing", () => {
    const { onSet } = mount(hud());
    fireEvent.change(input("Fighter UI", "x"), { target: { value: "1.5" } });
    expect(onSet).toHaveBeenCalledWith("fighter_x", "2");
  });

  test("an int field rounds half-down for a negative fraction the same way", () => {
    const { onSet } = mount(hud());
    fireEvent.change(input("Fighter UI", "x"), { target: { value: "3.4" } });
    expect(onSet).toHaveBeenCalledWith("fighter_x", "3");
  });

  test("a float field passes the raw text through unrounded", () => {
    const { onSet } = mount(hud());
    fireEvent.change(input("Ship HUD", "Offset from centre"), { target: { value: "-172.5" } });
    expect(onSet).toHaveBeenCalledWith("ship_offset", "-172.5");
  });

  test("a blank input writes nothing rather than a zero", () => {
    const { onSet } = mount(hud());
    fireEvent.change(input("Fighter UI", "x"), { target: { value: "   " } });
    expect(onSet).not.toHaveBeenCalled();
  });

  test("a non-numeric input writes nothing", () => {
    const { onSet } = mount(hud());
    fireEvent.change(input("Fighter UI", "x"), { target: { value: "abc" } });
    expect(onSet).not.toHaveBeenCalled();
  });

  test("a checkbox writes the string the backend parses, not a boolean", () => {
    const { onSet } = mount(hud());
    fireEvent.click(input("Fighter UI", "Detached"));
    expect(onSet).toHaveBeenCalledWith("fighter_detached", "true");
  });
});

describe("what the panel refuses to edit", () => {
  test("read-only disables every field", () => {
    mount(hud(), { readOnly: true });
    for (const input of document.querySelectorAll<HTMLInputElement>(".hud-panel input")) {
      expect(input.disabled).toBe(true);
    }
  });

  test("a field the file cannot hold is disabled on its own", () => {
    mount(hud(entry("badge_x", "int", null, { set: UNAVAILABLE })));
    expect(input("Fighter UI", "x").disabled).toBe(false);
    expect(input("Notification badge", "x").disabled).toBe(true);
  });

  test("a read-only ACCOUNT file disables only the rows that write it", () => {
    // The panel's `readOnly` is the character document's flag; four rows write
    // the account file instead, and were left clickable when that file was the
    // read-only one — the refusal then arrived as a backend dialog.
    mount(hud(), { accountReadOnly: true });
    expect(input("Neocom", "Width").disabled).toBe(true);
    expect(input("Fighter UI", "Detached").disabled).toBe(true);
    expect(input("Fighter UI", "x").disabled).toBe(false);
    expect(input("Ship HUD", "Offset from centre").disabled).toBe(false);
  });

  test("a fighter axis the file cannot hold is disabled without taking its sibling with it", () => {
    mount(hud(entry("fighter_y", "int", null, { set: UNAVAILABLE })));
    expect(input("Fighter UI", "y").disabled).toBe(true);
    expect(input("Fighter UI", "x").disabled).toBe(false);
  });
});

describe("what the panel shows", () => {
  test("an absent value falls back to EVE's default and is badged", () => {
    mount(hud(entry("neocom_width", "int", null, { scope: "account", default: "37" })));
    expect(input("Neocom", "Width").value).toBe("37");
    expect(row("Neocom", "Width").textContent).toContain("default");
  });

  test("account-scoped rows are badged so an account-wide write is visible", () => {
    mount(hud());
    expect(row("Fighter UI", "Detached").textContent).toContain("account");
    expect(row("Notification badge", "y").textContent).not.toContain("account");
  });

  test("the account legend names the other characters an edit would change", () => {
    mount(hud(), { sharedNames: ["Second Pilot", "Third Pilot"] });
    expect(document.querySelector(".account-legend")!.textContent).toContain(
      "Second Pilot, Third Pilot",
    );
  });

  test("the legend is hidden when no account row is actually writable", () => {
    mount(
      hud(
        entry("ship_top", "bool", null, { scope: "account", set: UNAVAILABLE }),
        entry("fighter_detached", "bool", null, { scope: "account", set: UNAVAILABLE }),
        entry("fighter_shown", "bool", null, { scope: "account", set: UNAVAILABLE }),
        entry("neocom_width", "int", null, { scope: "account", set: UNAVAILABLE }),
      ),
    );
    expect(document.querySelector(".account-legend")).toBeNull();
  });
});

test("clicking a group title selects that furniture on the canvas", () => {
  const { onSelectKind } = mount(hud());
  fireEvent.click(screen.getByRole("button", { name: "Notification badge" }));
  expect(onSelectKind).toHaveBeenCalledWith("badge");
});
