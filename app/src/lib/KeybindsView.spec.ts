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

/**
 * Capturing a binding swallows EVERY key, including the app's own accelerators —
 * `Ctrl+Z` while a chip is listening must bind Ctrl+Z, not undo the document.
 *
 * The capture handler's `stopPropagation` is what makes that true, and it is one
 * deletion away from being lost, so it gets its own test rather than riding on
 * the fact that nothing has broken it yet.
 */
test("Ctrl+Z while capturing binds the key instead of undoing", async () => {
  calls.stub("keybinds", BINDS);
  calls.stub("set_keybind", { keybinds: BINDS, stolen: [] });
  render(KeybindsView, { userOpen: true, userId: 1, onUserDirty: noop });

  const cell = await screen.findByTitle(TAKER);
  const chip = cell.closest("tr")!.querySelector(".chip") as HTMLButtonElement;
  await fireEvent.click(chip);

  let reachedWindow = false;
  const spy = () => (reachedWindow = true);
  window.addEventListener("keydown", spy);
  try {
    await fireEvent.keyDown(chip, { key: "z", keyCode: 90, ctrlKey: true });
  } finally {
    window.removeEventListener("keydown", spy);
  }

  expect(reachedWindow, "the capture must not let Ctrl+Z bubble to the shell").toBe(false);
  expect(calls.of("set_keybind").length).toBe(1);
});
