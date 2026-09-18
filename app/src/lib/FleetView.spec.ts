// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen, waitFor, within } from "@testing-library/svelte";
import FleetView from "$lib/FleetView.svelte";
import { calls } from "$lib/test/setup";
import { toasts } from "$lib/ui/toasts.svelte";
import type { ColourEntry, Fleet, HudEntry } from "$lib/api";
import { BROADCASTS } from "$lib/fleet";

const noop = () => {};

const field = (name: string, value: string | null, scope: "char" | "account" = "account", def = "1"): HudEntry => ({
  name, kind: "int", value, default: def, scope,
  set: value === null ? { how: "insert", parent: [], key: { kind: "int", text: "0" } as never } : { how: "set", path: [] },
});

export const FLEET: Fleet = {
  fields: [
    field("listen_show_own", "1", "account", "0"),
    field("listen_HealArmor", "0", "account", "0"),
    field("listen_Target", "1"),
    field("listen_WarpTo", null),
    field("formation", "0", "char", "0"),
    field("formation_size", "20000", "char", "20000"),
    field("formation_spacing", null, "char", "2000"),
    field("finder_group_only", "1", "char"),
  ],
  // Every type, absent with no default, then the five states under test.
  colours: BROADCASTS.map((b): ColourEntry => ({ broadcast: b.type, state: { state: "absent" }, default: null })).map((c) =>
    ({
      HealArmor: { ...c, state: { state: "set", rgb: [0.1, 0.6, 0.1] }, default: [0.1, 0.6, 0.1] },
      Target: { ...c, default: [0.75, 0.0, 0.0] },
      Location: { ...c, state: { state: "cleared" } },
      JumpTo: { ...c, state: { state: "unreadable" } },
    } as Record<string, ColourEntry>)[c.broadcast] ?? c,
  ),
  watchlist: [
    { char_id: 1001131163, rgb: [0.2, 0.5, 1.0] },
    { char_id: 90000001, rgb: null },
  ],
  palette: [
    ["yellow", [1.0, 0.7, 0.0]], ["red", [0.75, 0.0, 0.0]], ["blue", [0.2, 0.5, 1.0]],
  ],
  char_open: true,
  user_open: true,
};

export function mount(over: Partial<Fleet> = {}, props: Partial<{ charOpen: boolean; userOpen: boolean }> = {}) {
  const fleet = { ...FLEET, ...over };
  calls.stub("fleet_settings", fleet);
  calls.stub("set_fleet_field", fleet);
  calls.stub("set_fleet_colour", fleet);
  calls.stub("set_watchlist_colour", fleet);
  return render(FleetView, {
    charOpen: true, userOpen: true, userId: 1, charId: 2,
    onUserDirty: noop, onCharDirty: noop, ...props,
  });
}

// `findByRole("heading", …)` alone is racy: the heading renders unconditionally
// on mount, before the async `fleet_settings` reload resolves, so it can
// resolve on the very first (synchronous) check — before the panel has any
// content. Wait for a second top-level child (the loaded rows, the empty
// state, or the error message) beside the header before handing the section
// back, so every caller's subsequent synchronous query sees the loaded panel.
const panel = async (title: string) => {
  const el = (await screen.findByRole("heading", { name: title })).closest("section") as HTMLElement;
  await waitFor(() => expect(el.children.length).toBeGreaterThan(1));
  return el;
};

describe("the broadcast panel", () => {
  test("renders the top checkbox and every type in EVE's order, checked from the file", async () => {
    mount();
    const p = await panel("Broadcast settings");
    const boxes = within(p).getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes.length).toBe(17);
    expect(within(p).getByLabelText("Always show my own broadcasts")).toBeTruthy();
    expect((within(p).getByLabelText("Need armor") as HTMLInputElement).checked).toBe(false);
    expect((within(p).getByLabelText("Target") as HTMLInputElement).checked).toBe(true);
    // Never written: the default (1) is what shows.
    expect((within(p).getByLabelText("Warp to") as HTMLInputElement).checked).toBe(true);
    expect(within(p).getByText("account file")).toBeTruthy();
  });

  test("a toggle writes 1 or 0 to the type's field and marks the account dirty", async () => {
    let dirty = 0;
    calls.stub("fleet_settings", FLEET);
    calls.stub("set_fleet_field", FLEET);
    render(FleetView, { charOpen: true, userOpen: true, onUserDirty: () => dirty++, onCharDirty: noop });
    const p = await panel("Broadcast settings");
    await fireEvent.click(within(p).getByLabelText("Need armor"));
    await waitFor(() => expect(calls.of("set_fleet_field").length).toBe(1));
    expect(calls.only("set_fleet_field").args).toEqual({ name: "listen_HealArmor", text: "1" });
    await waitFor(() => expect(dirty).toBe(1));
  });

  test("the top checkbox writes its own field", async () => {
    mount();
    const p = await panel("Broadcast settings");
    await fireEvent.click(within(p).getByLabelText("Always show my own broadcasts"));
    await waitFor(() => expect(calls.only("set_fleet_field").args).toEqual({ name: "listen_show_own", text: "0" }));
  });

  test("swatches show the stored colour, the default for an absent one, and a picked palette hex writes exact floats", async () => {
    mount();
    const p = await panel("Broadcast settings");
    const armor = within(p).getByLabelText("Colour for Need armor") as HTMLInputElement;
    expect(armor.value).toBe("#1a991a"); // 0.1 * 255 rounds to 26
    const target = within(p).getByLabelText("Colour for Target") as HTMLInputElement;
    expect(target.value).toBe("#bf0000");
    await fireEvent.change(target, { target: { value: "#3380ff" } });
    await waitFor(() => expect(calls.of("set_fleet_colour").length).toBe(1));
    expect(calls.only("set_fleet_colour").args).toEqual({ broadcast: "Target", rgb: [0.2, 0.5, 1.0] });
  });

  test("✕ writes null, and is disabled once there is no colour", async () => {
    mount();
    const p = await panel("Broadcast settings");
    // By role+name, not `getByTitle`: the swatch itself also carries a "No
    // colour" title (the placeholder hint on an unset swatch), and that title
    // attribute is the one that survives when the ✕ button is disabled — its
    // own `title` swaps to the disabled reason then, but its `aria-label`
    // (which sets its accessible name, and is what this button is actually
    // found by) is the static "No colour" passed at the call site regardless
    // of state. Role scoping also excludes the swatch, which is not a button.
    const clearOn = (label: string) =>
      within(within(p).getByLabelText(label).closest(".row") as HTMLElement).getByRole("button", { name: "No colour" }) as HTMLButtonElement;
    // Set, and absent-with-a-default, can still be cleared; cleared, and
    // absent-with-no-default, cannot — there is nothing left for ✕ to do.
    expect(clearOn("Need armor").disabled).toBe(false);
    expect(clearOn("Target").disabled).toBe(false);
    expect(clearOn("At location").disabled).toBe(true);
    expect(clearOn("Warp to").disabled).toBe(true);
    expect(clearOn("Warp to").title).toBe("Already no colour");
    await fireEvent.click(clearOn("Need armor"));
    await waitFor(() => expect(calls.of("set_fleet_colour").length).toBe(1));
    expect(calls.only("set_fleet_colour").args).toEqual({ broadcast: "HealArmor", rgb: null });
  });

  test("an unreadable colour disables its swatch with a reason", async () => {
    mount();
    const p = await panel("Broadcast settings");
    const jump = within(p).getByLabelText("Colour for Jump to") as HTMLInputElement;
    expect(jump.disabled).toBe(true);
    expect(jump.title).toMatch(/unexpected type/);
  });

  test("a refused write reports on the panel in the error grammar", async () => {
    mount();
    calls.stub("set_fleet_field", () => Promise.reject({ code: "not_editable", message: "This value has an unexpected type here and cannot be edited safely." }));
    const p = await panel("Broadcast settings");
    await fireEvent.click(within(p).getByLabelText("Need armor"));
    const msg = await within(p).findByRole("alert");
    // `.trim()`: InlineMessage collapses the whitespace between its (absent)
    // `title` block and its children to one leading space when no title is
    // passed — the same reason other specs in this codebase read this text via
    // `.trim()` or `.toContain` rather than an unanchored match.
    expect(msg.textContent?.trim()).toMatch(/^That broadcast setting wasn't changed — This value has an unexpected type/);
    expect(msg.textContent).not.toMatch(/\[not_editable\]/);
  });

  test("no account file: an empty state with the pairing action", async () => {
    let shown = 0;
    calls.stub("fleet_settings", { ...FLEET, user_open: false, colours: [], fields: FLEET.fields.filter((f) => f.scope === "char") });
    render(FleetView, { charOpen: true, userOpen: false, onUserDirty: noop, onCharDirty: noop, onShowAccounts: () => shown++ });
    const p = await panel("Broadcast settings");
    expect(within(p).getByText("No account paired")).toBeTruthy();
    await fireEvent.click(within(p).getByRole("button", { name: "Pair this character…" }));
    expect(shown).toBe(1);
  });

  test("reloads when refreshToken changes", async () => {
    const { rerender } = mount();
    await panel("Broadcast settings");
    await waitFor(() => expect(calls.of("fleet_settings").length).toBe(1));
    await rerender({ charOpen: true, userOpen: true, userId: 1, charId: 2, refreshToken: 1, onUserDirty: noop, onCharDirty: noop });
    await waitFor(() => expect(calls.of("fleet_settings").length).toBe(2));
  });
});

describe("the watch list panel", () => {
  test("lists entries with the resolved name, the id, a swatch and a remove button", async () => {
    calls.stub("resolve_character_names", { "1001131163": { name: "Farm Delay", category: "character" } });
    mount();
    const p = await panel("Watch list colours");
    const row = (await within(p).findByText("Farm Delay")).closest("li")!;
    expect(within(row).getByText("1001131163")).toBeTruthy();
    expect((within(row).getByLabelText("Colour for Farm Delay") as HTMLInputElement).value).toBe("#3380ff");
    expect(within(p).getByText("character file")).toBeTruthy();
    // The unresolved id renders bare, and its unreadable colour is marked and disabled.
    const bare = within(p).getByText("90000001").closest("li")!;
    expect(within(bare).getByText("unreadable")).toBeTruthy();
    expect((within(bare).getByLabelText("Colour for 90000001") as HTMLInputElement).disabled).toBe(true);
  });

  test("recolour writes exact palette floats; remove writes null", async () => {
    mount();
    const p = await panel("Watch list colours");
    const swatch = (await within(p).findByLabelText("Colour for 1001131163")) as HTMLInputElement;
    await fireEvent.change(swatch, { target: { value: "#bf0000" } });
    await waitFor(() => expect(calls.of("set_watchlist_colour").length).toBe(1));
    expect(calls.only("set_watchlist_colour").args).toEqual({ charId: 1001131163, rgb: [0.75, 0.0, 0.0] });
    const row = swatch.closest("li")!;
    await fireEvent.click(within(row).getByTitle("Remove from the list"));
    await waitFor(() => expect(calls.of("set_watchlist_colour").length).toBe(2));
    expect(calls.of("set_watchlist_colour")[1].args).toEqual({ charId: 1001131163, rgb: null });
  });

  test("add looks the name up, writes the picked colour, clears the box and toasts", async () => {
    toasts.splice(0, toasts.length);
    calls.stub("lookup_character", { id: 2117000000, name: "New Pilot" });
    mount();
    const p = await panel("Watch list colours");
    const box = within(p).getByLabelText("Add a character") as HTMLInputElement;
    const add = within(p).getByRole("button", { name: "Add" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    await fireEvent.input(box, { target: { value: "new pilot" } });
    expect(add.disabled).toBe(false);
    await fireEvent.click(add);
    await waitFor(() => expect(calls.only("lookup_character").args).toEqual({ query: "new pilot" }));
    await waitFor(() => expect(calls.of("set_watchlist_colour").length).toBe(1));
    // Blue is the default swatch (863 of 1,382 corpus entries).
    expect(calls.only("set_watchlist_colour").args).toEqual({ charId: 2117000000, rgb: [0.2, 0.5, 1.0] });
    await waitFor(() => expect(box.value).toBe(""));
    // ToastHost lives in +layout, so the component test reads the store.
    expect(toasts.map((t) => t.message)).toContain("Added New Pilot");
  });

  test("add: no such character, unreachable ESI, and already listed each say so and write nothing", async () => {
    mount();
    const p = await panel("Watch list colours");
    const box = within(p).getByLabelText("Add a character");
    const submit = async (q: string) => {
      await fireEvent.input(box, { target: { value: q } });
      await fireEvent.click(within(p).getByRole("button", { name: "Add" }));
    };

    // `.trim()`: InlineMessage collapses the whitespace around its (absent)
    // title block to a leading/trailing space when no title is passed — the
    // same reason the broadcast panel's error test above reads this way.
    calls.stub("lookup_character", null);
    await submit("Nobody");
    expect((await within(p).findByRole("alert")).textContent?.trim()).toBe("No character called Nobody");

    calls.stub("lookup_character", () => Promise.reject({ code: "esi", message: "ESI status 502" }));
    await submit("Someone");
    await waitFor(() => expect(within(p).getByRole("alert").textContent?.trim()).toBe("Someone wasn't looked up — couldn't reach ESI"));

    calls.stub("lookup_character", { id: 1001131163, name: "Farm Delay" });
    await submit("Farm Delay");
    await waitFor(() => expect(within(p).getByRole("alert").textContent?.trim()).toBe("Farm Delay is already in the list"));
    calls.never("set_watchlist_colour");
  });

  test("an empty list has an empty state and the add row; no character file has neither", async () => {
    mount({ watchlist: [] });
    const p = await panel("Watch list colours");
    expect(within(p).getByText("No watch-list colours")).toBeTruthy();
    expect(within(p).getByLabelText("Add a character")).toBeTruthy();
  });

  test("no character file: the panel says so without an action", async () => {
    calls.stub("fleet_settings", { ...FLEET, char_open: false, watchlist: [] });
    render(FleetView, { charOpen: false, userOpen: true, onUserDirty: noop, onCharDirty: noop });
    const p = await panel("Watch list colours");
    expect(within(p).getByText("No character open")).toBeTruthy();
    expect(within(p).queryByLabelText("Add a character")).toBeNull();
  });
});

describe("the formation panel", () => {
  test("renders alongside the other two, each with its file chip", async () => {
    mount();
    await panel("Broadcast settings");
    await panel("Watch list colours");
    const p = await panel("Formation");
    expect(within(p).getByText("character file")).toBeTruthy();
    // And with no account file, the character panels still render.
    calls.stub("fleet_settings", { ...FLEET, user_open: false });
    render(FleetView, { charOpen: true, userOpen: false, onUserDirty: noop, onCharDirty: noop });
    expect((await screen.findAllByRole("heading", { name: "Formation" })).length).toBe(2);
  });

  test("shows the three numbers and the finder toggle, defaults where absent", async () => {
    mount();
    const p = await panel("Formation");
    expect((within(p).getByLabelText("Formation") as HTMLInputElement).value).toBe("0");
    expect((within(p).getByLabelText("Size") as HTMLInputElement).value).toBe("20000");
    expect((within(p).getByLabelText("Spacing") as HTMLInputElement).value).toBe("2000");
    expect((within(p).getByLabelText("Show only my corp, alliance and high-standing fleets") as HTMLInputElement).checked).toBe(true);
    expect(within(p).getByText("character file")).toBeTruthy();
    expect(within(p).getByText(/Saved fleet setups live on CCP's servers/)).toBeTruthy();
  });

  test("a number commits rounded on change and the toggle writes 1/0", async () => {
    mount();
    const p = await panel("Formation");
    const size = within(p).getByLabelText("Size") as HTMLInputElement;
    await fireEvent.change(size, { target: { value: "30000.6" } });
    await waitFor(() => expect(calls.only("set_fleet_field").args).toEqual({ name: "formation_size", text: "30001" }));
    await fireEvent.click(within(p).getByLabelText("Show only my corp, alliance and high-standing fleets"));
    await waitFor(() => expect(calls.of("set_fleet_field").length).toBe(2));
    expect(calls.of("set_fleet_field")[1].args).toEqual({ name: "finder_group_only", text: "0" });
  });

  test("a refused number edit reports on this panel and puts the field back", async () => {
    mount();
    calls.stub("set_fleet_field", () => Promise.reject({ code: "not_editable", message: "nope" }));
    const p = await panel("Formation");
    const size = within(p).getByLabelText("Size") as HTMLInputElement;
    await fireEvent.change(size, { target: { value: "1" } });
    // `.trim()`: same InlineMessage whitespace-collapse as the broadcast and
    // watch-list panels' own error tests above (no `title` prop passed here).
    expect((await within(p).findByRole("alert")).textContent?.trim()).toBe("That formation setting wasn't changed — nope");
    await waitFor(() => expect(size.value).toBe("20000"));
  });
});
