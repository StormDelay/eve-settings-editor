// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import KeybindsView from "$lib/KeybindsView.svelte";
import { calls } from "$lib/test/setup";
import type { Keybinds } from "$lib/api";

const TAKER = "CmdActivateHighPowerSlot4"; // catalog label: "Activate High Power Slot 4"
const VICTIM = "CmdActivateHighPowerSlot5";

const BINDS: Keybinds = {
  available: true,
  entries: [
    { command: TAKER, keys: [113], malformed: false },
    { command: VICTIM, keys: [114], malformed: false },
  ],
};

const noop = () => {};

/// Rebind TAKER onto a key VICTIM held, which is what puts the "taken by" note
/// on VICTIM's row — it is the only path that renders one.
async function steal() {
  calls.stub("keybinds", BINDS);
  calls.stub("set_keybind", { keybinds: BINDS, stolen: [VICTIM] });
  render(KeybindsView, { userOpen: true, userId: 1, onUserDirty: noop });

  const cell = await screen.findByTitle(TAKER);
  const chip = cell.closest("tr")!.querySelector(".chip") as HTMLButtonElement;
  await fireEvent.click(chip);
  await fireEvent.keyDown(chip, { key: "q", keyCode: 81 });
}

describe("the 'taken by' note", () => {
  test("a long note keeps the full command on its title", async () => {
    // The visible text is ellipsised to keep the row's height fixed, so the
    // untruncated name has to remain reachable — otherwise constraining the row
    // silently destroys the information it was showing.
    await steal();
    const note = await screen.findByText(/taken by/);
    expect(note.getAttribute("title")).toBe("Activate High Power Slot 4");
  });
});
