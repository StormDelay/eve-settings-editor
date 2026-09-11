// Component test (vitest + jsdom).
//
// Everything this pane shows was previously invisible, hover-only, or encoded
// as a text colour: the node's kind, its raw edit text, its path, and the
// "inside a shared object" warning that was a single "&" glyph.
//
// The load-bearing behaviour is the resolution: it holds a PATH and finds the
// node in the CURRENT tree, so an edit that rebuilds the tree cannot leave it
// showing a stale value.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import RawInspector from "$lib/RawInspector.svelte";
import type { NodePath, TreeNodeData } from "$lib/api";

function node(over: Partial<TreeNodeData> = {}): TreeNodeData {
  return {
    label: null,
    kind: "int",
    display: "42",
    path: [],
    editable: true,
    edit_text: "42",
    removable: false,
    in_shared: false,
    children: [],
    ...over,
  };
}

// root → dict_value[0] → list[2]
const leaf = node({
  label: "width",
  kind: "int",
  display: "180",
  edit_text: "180",
  removable: true,
  path: [{ s: "dict_value", i: 0 }, { s: "list", i: 2 }],
});
const mid = node({
  label: "columns",
  kind: "list",
  display: "[3]",
  edit_text: null,
  path: [{ s: "dict_value", i: 0 }],
  children: [leaf],
});
const root = node({ label: "root", kind: "dict", display: "{1}", edit_text: null, path: [], children: [mid] });

function mount(path: NodePath | null, over: Record<string, unknown> = {}) {
  const spies = { onReveal: vi.fn(), onRemove: vi.fn(), onInsertRequest: vi.fn() };
  render(RawInspector, { root, path, file: "char", ...spies, ...over } as never);
  return spies;
}

/** Mount on a one-node tree, for the cases that are about that node alone. */
function only(n: TreeNodeData) {
  return mount(n.path, { root: n });
}

const fieldValue = (label: string) => (screen.getByLabelText(label) as HTMLInputElement).value;

test("with nothing selected it says what to do", () => {
  mount(null);
  expect(screen.getByText(/select a node/i)).toBeTruthy();
});

describe("resolving the selection", () => {
  test("finds the node the path names, at any depth", () => {
    mount(leaf.path);
    expect(fieldValue("Value")).toBe("180");
  });

  // The whole reason it holds a path: a removed or filtered-away node must not
  // leave the pane asserting a value the file no longer has.
  test("a path the current tree no longer has resolves to nothing", () => {
    mount([{ s: "dict_value", i: 0 }, { s: "list", i: 9 }]);
    expect(screen.getByText(/select a node/i)).toBeTruthy();
  });
});

describe("what it shows", () => {
  // The tree paints six kinds in six colours and says the word nowhere.
  test("the kind is a word, not a colour", () => {
    mount(mid.path);
    expect(fieldValue("Type")).toBe("list");
  });

  test("a kind whose name is not the word gets the word", () => {
    mount(root.path);
    expect(fieldValue("Type")).toBe("dictionary");
  });

  test("the path is spelled out, step by step", () => {
    mount(leaf.path);
    expect(fieldValue("Path")).toBe("dict_value[0] › list[2]");
  });

  test("the raw text is hidden when it is the rendered value", () => {
    mount(leaf.path);
    expect(screen.queryByLabelText("Raw value")).toBeNull();
  });

  test("the raw text is shown when the two differ", () => {
    only(node({ display: "«bytes, 12»", edit_text: "0x0102" }));
    expect(fieldValue("Raw value")).toBe("0x0102");
  });

  // Was a single "&" beside the value.
  test("a shared object carries the warning in words", () => {
    only(node({ in_shared: true }));
    expect(screen.getByText(/changes every place it is referenced/i)).toBeTruthy();
  });

  test("which file the node is in", () => {
    mount(leaf.path, { file: "user" });
    expect(screen.getByText("account file")).toBeTruthy();
  });
});

// The three row actions were `opacity: 0` until the row was hovered.
describe("the row actions, as real buttons", () => {
  test("a container offers Add entry", async () => {
    const { onInsertRequest } = mount(mid.path);
    await fireEvent.click(screen.getByRole("button", { name: "Add entry…" }));
    expect(onInsertRequest).toHaveBeenCalledWith(mid);
  });

  test("a removable node offers Remove, and a scalar offers no Add", async () => {
    const { onRemove } = mount(leaf.path);
    expect(screen.queryByRole("button", { name: "Add entry…" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Remove entry" }));
    expect(onRemove).toHaveBeenCalledWith(leaf.path);
  });

  test("Show in tree reveals the selected node", async () => {
    const { onReveal } = mount(leaf.path);
    await fireEvent.click(screen.getByRole("button", { name: "Show in tree" }));
    expect(onReveal).toHaveBeenCalledWith(leaf.path);
  });
});
