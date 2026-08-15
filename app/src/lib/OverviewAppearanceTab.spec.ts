// Component test: vitest + jsdom, Tauri's `invoke` stubbed by $lib/test/setup.
//
// Two behaviours worth pinning, both about what reaches the FILE:
//   1. Colortag colours exist. `stateColors` is keyed by `(surface, id)` and the
//      editor read `background` only, so a pack's `flag_*` colours (Z-S sets
//      `flag_48`) were written into the account with no control able to show or
//      undo them. The swatch is no longer gated on the Background sub-tab, and
//      it must address the surface the user is looking at.
//   2. A picked hex inverts to n/255, which `overview_pack::color_name` cannot
//      match, so an off-palette colour is silently dropped from a pack export.
//      Picking a palette colour snaps to EVE's exact floats; anything else says
//      so on the row.
import { describe, expect, test } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import OverviewAppearanceTab from "$lib/OverviewAppearanceTab.svelte";
import { calls } from "$lib/test/setup";
import type { OverviewColumns } from "$lib/api";

// States 13 (criminal) and 48 (fleet member) both carry labels, so both draw a
// row. Background 13 holds the palette's `red`; colortag 48 holds a colour no
// palette name matches.
const data: OverviewColumns = {
  tabs: [],
  windows: [],
  presets: [],
  appearance: {
    background: { enabled: [13], order: [13, 48] },
    flag: { enabled: [48], order: [13, 48] },
    colors: [[13, [0.75, 0.0, 0.0, 1.0]]],
    flag_colors: [[48, [0.1, 0.2, 0.3, 1.0]]],
    palette: [
      ["red", [0.75, 0.0, 0.0, 1.0]],
      ["white", [0.7, 0.7, 0.7, 1.0]],
    ],
    bools: [],
    defaulted: false,
  },
};

const noop = () => {};

function mount() {
  calls.stub("overview_set_state_color", data);
  render(OverviewAppearanceTab, { data, onChanged: noop, onUserDirty: noop });
}

const swatches = () =>
  [...document.querySelectorAll<HTMLInputElement>(".state-list input[type='color']")];

/** Switch to the Colortag sub-tab. */
async function colortag() {
  const tab = [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Colortag");
  if (!tab) throw new Error("no Colortag sub-tab");
  await fireEvent.click(tab);
  // The strip is a roving tabindex; the click is what selects.
  await waitFor(() => expect(tab.getAttribute("aria-selected")).toBe("true"));
}

describe("the colortag surface", () => {
  test("offers a colour control, showing the stored flag colour", async () => {
    mount();
    await colortag();
    // Both rows draw a swatch — the control is no longer Background-only.
    expect(swatches().length).toBe(2);
    // Row order is the order list [13, 48]; 48 is the one with a stored colour.
    expect(swatches()[1].value).toBe("#1a334d");
  });

  test("writes to the flag surface, not the background one", async () => {
    mount();
    await colortag();
    const s = swatches()[1];
    await fireEvent.change(s, { target: { value: "#bf0000" } });
    await waitFor(() => expect(calls.of("overview_set_state_color").length).toBe(1));
    expect(calls.only("overview_set_state_color").args).toEqual({
      surface: "flag",
      id: 48,
      rgba: [0.75, 0.0, 0.0, 1.0],
    });
  });

  test("Reset clears the flag entry, not the background entry", async () => {
    mount();
    await colortag();
    const reset = [...document.querySelectorAll("button")].filter((b) => b.textContent?.trim() === "Reset");
    // Only state 48 has a stored colortag colour, so only its row offers Reset.
    expect(reset.length).toBe(1);
    await fireEvent.click(reset[0]);
    await waitFor(() => expect(calls.of("overview_set_state_color").length).toBe(1));
    expect(calls.only("overview_set_state_color").args).toEqual({
      surface: "flag",
      id: 48,
      rgba: null,
    });
  });
});

describe("colours a pack export cannot name", () => {
  // #bf0000 inverts to 0.74901…, which `color_name` will not match. Writing the
  // inversion would drop the colour from an export; the palette's own floats
  // are what survive it.
  test("a picked palette colour is snapped to EVE's exact floats", async () => {
    mount();
    await fireEvent.change(swatches()[1], { target: { value: "#bf0000" } });
    await waitFor(() => expect(calls.of("overview_set_state_color").length).toBe(1));
    expect(calls.only("overview_set_state_color").args).toEqual({
      surface: "background",
      id: 48,
      rgba: [0.75, 0.0, 0.0, 1.0],
    });
  });

  test("an off-palette colour is still written, as picked", async () => {
    mount();
    await fireEvent.change(swatches()[1], { target: { value: "#010203" } });
    await waitFor(() => expect(calls.of("overview_set_state_color").length).toBe(1));
    expect(calls.only("overview_set_state_color").args).toEqual({
      surface: "background",
      id: 48,
      rgba: [1 / 255, 2 / 255, 3 / 255, 1.0],
    });
  });

  test("a stored colour with no palette name is marked, one the palette names is not", async () => {
    mount();
    // Background: state 13 is the palette's `red`.
    expect(document.querySelectorAll(".off-palette").length).toBe(0);
    await colortag();
    // Colortag: state 48's (0.1, 0.2, 0.3) matches no name.
    expect(document.querySelectorAll(".off-palette").length).toBe(1);
  });

  test("the palette is offered to the picker as suggestions", () => {
    mount();
    expect(document.querySelectorAll("#eve-palette option").length).toBe(2);
    expect(swatches()[0].getAttribute("list")).toBe("eve-palette");
  });
});
