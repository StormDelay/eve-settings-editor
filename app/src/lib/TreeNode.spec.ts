// Component test (vitest + jsdom).
//
// The tree had no selection at all, which is why its per-node metadata had
// nowhere to be shown and lived as a text colour and a one-character glyph.
// Single click now selects; double click still opens the inline editor, and
// that pairing is the thing worth pinning — a browser fires two `click`s before
// a `dblclick`, so the two gestures share the same element.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import TreeNode from "$lib/TreeNode.svelte";
import type { TreeNodeData } from "$lib/api";

const node: TreeNodeData = {
  label: "width",
  kind: "int",
  display: "180",
  path: [{ s: "dict_value", i: 0 }],
  editable: true,
  edit_text: "180",
  removable: false,
  in_shared: false,
  children: [],
};

function mount(over: Record<string, unknown> = {}) {
  const spies = {
    onSelect: vi.fn(),
    onReveal: vi.fn(),
    onEdit: vi.fn().mockResolvedValue(undefined),
    onRemove: vi.fn().mockResolvedValue(undefined),
    onInsertRequest: vi.fn(),
  };
  render(TreeNode, { node, ...spies, ...over } as never);
  return spies;
}

const value = () => screen.getByText("180");

describe("selection", () => {
  test("clicking the value selects the node", async () => {
    const { onSelect } = mount();
    await fireEvent.click(value());
    expect(onSelect).toHaveBeenCalledWith(node);
  });

  test("Enter selects it too", async () => {
    const { onSelect } = mount();
    await fireEvent.keyDown(value(), { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith(node);
  });

  test("the selected node is marked, and only it", () => {
    mount({ selectedPath: node.path });
    expect(value().getAttribute("aria-pressed")).toBe("true");
  });

  test("a different path leaves it unmarked", () => {
    mount({ selectedPath: [{ s: "dict_value", i: 1 }] });
    expect(value().getAttribute("aria-pressed")).toBe("false");
  });

  // Selecting must not have cost the editor: the click handler was added to the
  // element that already carried the double-click.
  test("double-clicking still opens the inline editor", async () => {
    mount();
    await fireEvent.dblClick(value());
    expect(screen.getByLabelText("Edit value")).toBeTruthy();
  });

  test("a node that is not editable does not open one", async () => {
    mount({ node: { ...node, editable: false } });
    await fireEvent.dblClick(value());
    expect(screen.queryByLabelText("Edit value")).toBeNull();
  });
});
