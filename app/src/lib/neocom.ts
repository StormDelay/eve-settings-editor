// Pure helpers for the neocom button list. No DOM, no Svelte — unit-tested in
// neocom.test.ts.
import type { NeocomButton } from "./api";

export interface CatalogButton {
  id: string;
  btnType: number;
  iconPath: string;
}

export interface Addable extends CatalogButton {
  /** Where this button's btnType/iconPath came from. A button the character's
   * own client wrote is more trustworthy than one the bundled catalog supplied. */
  source: "original" | "catalog";
}

/**
 * Buttons the user can add: the character's own `neocomButtonRawDataOriginal`
 * unioned with the bundled catalog, minus whatever is already on the bar.
 *
 * Both halves are needed. Original is a stale snapshot — only ~12% of corpus
 * characters have a bar that is a subset of it, and nine common ids (fleet,
 * accessgroups, corporation, …) appear on bars that Original never listed. The
 * catalog covers those. Original covers the reverse case: a client that knows a
 * button this catalog does not.
 */
export function addableButtons(
  onBar: NeocomButton[],
  original: NeocomButton[],
  catalog: CatalogButton[],
): Addable[] {
  const taken = new Set(onBar.map((b) => b.id));
  const by = new Map<string, Addable>();
  for (const c of catalog) {
    if (!taken.has(c.id)) by.set(c.id, { ...c, source: "catalog" });
  }
  // Original last: it overwrites the catalog entry, because it came from this
  // character's own client.
  for (const o of original) {
    if (!taken.has(o.id)) {
      by.set(o.id, { id: o.id, btnType: o.btn_type, iconPath: o.icon_path, source: "original" });
    }
  }
  return [...by.values()].sort((a, b) => a.id.localeCompare(b.id));
}
