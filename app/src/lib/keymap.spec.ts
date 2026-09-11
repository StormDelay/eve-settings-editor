// The keyboard map, tested without a DOM where it can be.
//
// The one thing worth pinning about a keyboard map is which key means what, and
// that a global accelerator does not steal a key the field under the caret
// needs — a stolen Ctrl+A is a worse bug than a missing shortcut.
import { describe, expect, test, vi } from "vitest";
import { commandFor, handleKey, inAField } from "$lib/keymap";
import type { Ctx } from "$lib/commands";
import { resetSubject, subject } from "$lib/subject.svelte";

const noop = () => {};
const ctx = (over: Partial<Ctx> = {}): Ctx => ({
  goto: noop,
  pickFile: noop,
  save: noop,
  discard: noop,
  showHistory: noop,
  showAccounts: noop,
  showBatch: noop,
  showAbout: noop,
  showShortcuts: noop,
  openPalette: noop,
  findInView: noop,
  ...over,
});

/** A keydown carrying the platform's primary modifier, as the app matches it. */
const chord = (key: string, target: EventTarget | null = null) =>
  ({
    key,
    ctrlKey: true,
    metaKey: false,
    shiftKey: false,
    target,
    preventDefault: vi.fn(),
  }) as unknown as KeyboardEvent;

describe("which key means what", () => {
  test.each([
    ["o", "file.open"],
    ["s", "file.save"],
    ["h", "file.history"],
    ["k", "palette.open"],
    ["f", "view.find"],
    ["/", "help.shortcuts"],
    ["1", "go.layout"],
    ["6", "go.raw"],
  ])("%s is %s", (key, id) => {
    expect(commandFor(chord(key))?.id).toBe(id);
  });

  test("the view digits are the tab strip's own order", () => {
    const ids = ["1", "2", "3", "4", "5", "6"].map((k) => commandFor(chord(k))!.id);
    expect(ids).toEqual(["go.layout", "go.overview", "go.autofill", "go.keybinds", "go.probes", "go.raw"]);
  });

  test("a bare key is not a chord", () => {
    expect(commandFor({ key: "s", ctrlKey: false, metaKey: false } as KeyboardEvent)).toBeNull();
  });

  test("an unbound chord is left to the webview", () => {
    expect(commandFor(chord("q"))).toBeNull();
  });

  test("case does not matter — Ctrl+Shift+S is still Save's key", () => {
    expect(commandFor(chord("S"))?.id).toBe("file.save");
  });
});

describe("fields keep the keys they need", () => {
  const input = document.createElement("input");
  const div = document.createElement("div");

  test("inAField recognises the three editable controls", () => {
    expect(inAField(input)).toBe(true);
    expect(inAField(document.createElement("textarea"))).toBe(true);
    expect(inAField(document.createElement("select"))).toBe(true);
    expect(inAField(div)).toBe(false);
    expect(inAField(null)).toBe(false);
  });

  /** Ctrl+O inside a text box must reach the box, not open a file picker over
   *  whatever the user was halfway through typing. */
  test("a chord fired from inside a field is not consumed", () => {
    let opened = false;
    expect(handleKey(chord("o", input), ctx({ pickFile: () => (opened = true) }))).toBe(false);
    expect(opened).toBe(false);
  });

  /** The three exceptions, and the reason each is one: Find and the palette are
   *  how you GET to a field and replace its content wholesale rather than
   *  editing it, and Save has to work from anywhere or it is not a Save key. */
  test("Find and the palette still work from inside a field", () => {
    for (const [key, spy] of [
      ["f", "findInView"],
      ["k", "openPalette"],
    ] as const) {
      let hit = false;
      expect(handleKey(chord(key, input), ctx({ [spy]: () => (hit = true) }))).toBe(true);
      expect(hit, key).toBe(true);
    }
  });

  /** Save reaches the map from inside a field too — it is simply disabled here,
   *  because nothing is open. Consumed either way: whether Save fires is
   *  `canSave`'s business, not the caret's. */
  test("Save reaches the map from inside a field", () => {
    resetSubject();
    const e = chord("s", input);
    expect(handleKey(e, ctx())).toBe(true);
    expect(e.preventDefault).toHaveBeenCalled();
  });
});

describe("running, and refusing to run", () => {
  test("an enabled command runs and the event is consumed", () => {
    resetSubject();
    let where: string | null = null;
    const e = chord("6");
    expect(handleKey(e, ctx({ goto: (v) => (where = v) }))).toBe(true);
    expect(where).toBe("raw");
    expect(e.preventDefault).toHaveBeenCalled();
  });

  /**
   * A disabled command is consumed and does nothing. Not an error, and not a
   * fall-through: the app has claimed the key, so letting the webview do
   * something arbitrary with it would be worse than the no-op.
   */
  test("a disabled command is swallowed rather than run", () => {
    resetSubject();
    let saved = false;
    const e = chord("s");
    expect(handleKey(e, ctx({ save: () => (saved = true) }))).toBe(true);
    expect(saved).toBe(false);
    expect(e.preventDefault).toHaveBeenCalled();
  });

  test("the same command runs once it is enabled", () => {
    resetSubject();
    subject.slots.char = {
      status: "opened",
      path: "/eve/core_char_950.dat",
      file_name: "core_char_950.dat",
      fidelity: { state: "editable" },
      tree: { label: "root", kind: "dict", display: "{}", path: [], editable: false, edit_text: null, removable: false, in_shared: false, children: [] },
    };
    subject.dirty.char = true;
    let saved = false;
    expect(handleKey(chord("s"), ctx({ save: () => (saved = true) }))).toBe(true);
    expect(saved).toBe(true);
    resetSubject();
  });
});
