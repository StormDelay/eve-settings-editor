// The frontend half of undo.
//
// The backend's own tests prove the stack is correct. These prove the three
// things that can only go wrong up here: that the frontend keeps no opinion
// about what is unsaved, that a text field keeps its own undo, and that an
// empty stack SAYS SO rather than doing nothing visible.
import { beforeEach, describe, expect, test, vi } from "vitest";
import { calls } from "$lib/test/setup";
import { doRedo, doUndo, noteEdit, undoAction, undoState } from "$lib/undo.svelte";
import { handleKey, inAField } from "$lib/keymap";
import { resetSubject, subject } from "$lib/subject.svelte";
import { toasts } from "$lib/ui/toasts.svelte";
import type { Ctx } from "$lib/commands";
import type { OpenOutcome, TreeNodeData } from "$lib/api";

const tree = (display: string): TreeNodeData => ({
  label: "root",
  kind: "dict",
  display,
  path: [],
  editable: false,
  edit_text: null,
  removable: false,
  in_shared: false,
  children: [],
});

const opened = (file_name: string, display = "{}"): OpenOutcome => ({
  status: "opened",
  path: `/eve/${file_name}`,
  file_name,
  fidelity: { state: "editable" },
  tree: tree(display),
});

const outcome = (over: Record<string, unknown> = {}) => ({
  char_tree: tree("reverted"),
  user_tree: null,
  dirty: { char: false, user: false },
  state: { can_undo: false, can_redo: true, depth: 0 },
  ...over,
});

beforeEach(() => {
  resetSubject();
  toasts.length = 0;
  undoState.canUndo = false;
  undoState.canRedo = false;
});

describe("what an undo does to the shell", () => {
  test("the reverted tree replaces the open one", async () => {
    subject.slots.char = opened("core_char_950.dat", "before");
    calls.stub("undo", outcome());

    await doUndo();

    const doc = subject.slots.char;
    expect(doc?.status === "opened" && doc.tree.display).toBe("reverted");
  });

  /**
   * The unsaved badges come from the RESPONSE, and the frontend keeps no
   * opinion. After edit → save → edit → undo the file is clean; after one more
   * undo it is dirty again, and only something that knows where the save point
   * sits can tell those two apart.
   */
  test("the unsaved badges are taken from the response, not guessed", async () => {
    subject.slots.char = opened("core_char_950.dat");
    subject.dirty.char = true;
    subject.dirty.user = true;
    calls.stub("undo", outcome({ dirty: { char: false, user: false } }));

    await doUndo();

    expect(subject.dirty.char).toBe(false);
    expect(subject.dirty.user).toBe(false);
  });

  test("undoing PAST the save point sets the badge again", async () => {
    subject.slots.char = opened("core_char_950.dat");
    subject.dirty.char = false;
    calls.stub("undo", outcome({ dirty: { char: true, user: false } }));

    await doUndo();

    expect(subject.dirty.char).toBe(true);
  });

  /** Every view keys its reload on this, so an undo has to bump it or Autofill,
   *  Keybinds and Probes would show the pre-undo document. */
  test("it bumps the token every view reloads on", async () => {
    subject.slots.char = opened("core_char_950.dat");
    calls.stub("undo", outcome());
    const before = subject.savedAt;

    await doUndo();

    expect(subject.savedAt).toBe(before + 1);
  });

  test("can-undo and can-redo come from the response too", async () => {
    subject.slots.char = opened("core_char_950.dat");
    calls.stub("undo", outcome({ state: { can_undo: true, can_redo: true, depth: 3 } }));

    await doUndo();

    expect(undoState.canUndo).toBe(true);
    expect(undoState.canRedo).toBe(true);
  });
});

describe("an empty stack", () => {
  /** Never silence. Silence after a delete is exactly how a user concludes the
   *  undo did something they cannot see. */
  test("Ctrl+Z with nothing to undo says so", async () => {
    calls.stub("undo", null);
    await doUndo();
    expect(toasts.map((t) => t.message)).toContain("Nothing to undo.");
  });

  test("and so does redo", async () => {
    calls.stub("redo", null);
    await doRedo();
    expect(toasts.map((t) => t.message)).toContain("Nothing to redo.");
  });

  test("nothing is applied to the shell", async () => {
    subject.slots.char = opened("core_char_950.dat", "untouched");
    calls.stub("undo", null);
    const before = subject.savedAt;

    await doUndo();

    const doc = subject.slots.char;
    expect(doc?.status === "opened" && doc.tree.display).toBe("untouched");
    expect(subject.savedAt).toBe(before);
  });
});

describe("the text-field guard", () => {
  const noop = () => {};
  const ctx: Ctx = {
    goto: noop, pickFile: noop, save: noop, discard: noop, showHistory: noop,
    showAccounts: noop, showBatch: noop, showAbout: noop, showShortcuts: noop,
    openPalette: noop, findInView: noop,
  };
  const chord = (key: string, target: EventTarget | null, shiftKey = false) =>
    ({ key, ctrlKey: true, metaKey: false, shiftKey, target, preventDefault: vi.fn() }) as unknown as KeyboardEvent;

  /** Mandatory rather than polish: a Ctrl+Z inside an input must reach the
   *  webview's own text undo, or typing a name becomes unrecoverable. */
  test("Ctrl+Z inside a text input does not reach the document stack", () => {
    calls.stub("undo", outcome());
    const input = document.createElement("input");
    expect(inAField(input)).toBe(true);
    expect(handleKey(chord("z", input), ctx)).toBe(false);
    calls.never("undo");
  });

  test("Ctrl+Z outside a field undoes", () => {
    calls.stub("undo", outcome());
    const e = chord("z", document.createElement("div"));
    expect(handleKey(e, ctx)).toBe(true);
    expect(e.preventDefault).toHaveBeenCalled();
  });

  /** Both redo bindings, because the app ships on three platforms: Ctrl+Y is
   *  the Windows convention and Cmd+Shift+Z the macOS one. */
  test("Ctrl+Shift+Z and Ctrl+Y both redo", () => {
    calls.stub("redo", outcome());
    const div = document.createElement("div");
    expect(handleKey(chord("z", div, true), ctx)).toBe(true);
    expect(handleKey(chord("y", div), ctx)).toBe(true);
  });
});

describe("the toast's Undo button", () => {
  /**
   * The hazard, pinned. The button undoes the TOP of the stack, so a later edit
   * moves what it points at — and it would then revert the newer change while
   * naming the older one.
   */
  test("it refuses once a later edit has moved the stack", async () => {
    calls.stub("undo_state", { can_undo: true, can_redo: false, depth: 1 });
    const action = undoAction()!;
    await Promise.resolve();

    // A later edit: the stack is deeper than when the toast was minted.
    calls.stub("undo_state", { can_undo: true, can_redo: false, depth: 2 });
    calls.stub("undo", outcome());
    action.run();
    await vi.waitFor(() =>
      expect(toasts.map((t) => t.message)).toContainEqual(
        expect.stringMatching(/no longer the most recent|isn't the most recent/i),
      ),
    );
    calls.never("undo");
  });

  test("it undoes when the step it names is still the top", async () => {
    calls.stub("undo_state", { can_undo: true, can_redo: false, depth: 1 });
    subject.slots.char = opened("core_char_950.dat");
    const action = undoAction()!;
    calls.stub("undo", outcome());

    action.run();
    await vi.waitFor(() => expect(calls.of("undo").length).toBe(1));
  });
});

describe("noteEdit", () => {
  test("an edit lights the Undo control without a round trip", () => {
    expect(undoState.canUndo).toBe(false);
    noteEdit();
    expect(undoState.canUndo).toBe(true);
    // A new edit forks the history, so redo is gone.
    expect(undoState.canRedo).toBe(false);
  });
});
