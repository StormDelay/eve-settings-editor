// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { searchTree } from "./search.ts";
import type { TreeNodeData } from "./api.ts";

import { check } from "./test/check.ts";

const node = (
  label: string | null,
  display: string,
  children: TreeNodeData[] = [],
): TreeNodeData => ({
  label,
  display,
  kind: children.length > 0 ? "dict" : "str",
  path: [],
  editable: false,
  edit_text: null,
  removable: false,
  in_shared: false,
  children,
});

const tree = node("root", "dict (2)", [
  node('b"ui"', "dict (2)", [
    node('b"chatWindows"', "list (1)", [node("[0]", '"Local"')]),
    node('b"colour"', '"red"'),
  ]),
  node('b"windows"', "dict (1)", [node('b"size"', "1920")]),
]);

const blank = searchTree(tree, "  ");
check("a blank query returns the tree untouched", blank.tree === tree);
check("a blank query counts nothing", blank.count === 0);

const hit = searchTree(tree, "chat");
check("finds the one match", hit.count === 1);
check("prunes the branch with no match", hit.tree?.children.length === 1);
const ui = hit.tree!.children[0];
check("keeps the path down to the match", ui.label === 'b"ui"');
check("prunes non-matching siblings", ui.children.length === 1);
check(
  "a match keeps its whole subtree, so it can be drilled into",
  ui.children[0].children.length === 1,
);

check("matches values, not just labels", searchTree(tree, "1920").count === 1);
check("is case-insensitive", searchTree(tree, "LOCAL").count === 1);
check("counts matches nested inside a match", searchTree(tree, "o").count > 1);

const miss = searchTree(tree, "zzz-nothing");
check("no match yields no tree", miss.tree === null);
check("no match counts nothing", miss.count === 0);

