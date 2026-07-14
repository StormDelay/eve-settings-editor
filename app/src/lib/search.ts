// Tree search. The webview's own Ctrl+F only sees rendered DOM, and collapsed
// nodes are not in it — so a settings file is mostly unsearchable that way.
// This filters the projected tree instead, which holds every node.
import type { TreeNodeData } from "./api";

export interface SearchResult {
  /// The tree pruned to matches and the ancestors leading to them; null when
  /// nothing matched.
  tree: TreeNodeData | null;
  count: number;
}

/**
 * Case-insensitive substring search over each node's label and its rendered
 * value, so `chat`, `1920` and `b"overview"` all find what you would expect.
 * A matching node keeps its whole subtree — you land on the hit and can drill
 * into it. Non-matching nodes survive only as the path to a hit.
 */
export function searchTree(root: TreeNodeData, query: string): SearchResult {
  const q = query.trim().toLowerCase();
  if (q === "") return { tree: root, count: 0 };

  let count = 0;
  const walk = (n: TreeNodeData): TreeNodeData | null => {
    const hit = `${n.label ?? ""} ${n.display}`.toLowerCase().includes(q);
    if (hit) count += 1;
    // Descend even into a hit, so nested hits are counted too.
    const children = n.children
      .map(walk)
      .filter((c): c is TreeNodeData => c !== null);
    if (hit) return n;
    return children.length > 0 ? { ...n, children } : null;
  };

  return { tree: walk(root), count };
}
