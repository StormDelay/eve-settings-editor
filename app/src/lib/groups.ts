// Pure helpers for the overview preset-contents catalog: merge the bundled static
// group tree with ESI-synced additions, filter by name, and toggle membership.
// No Svelte/Tauri/JSON-import deps, so this is node --test-able. The component
// imports the bundle JSON and the backend additions and passes them in.

export interface CatGroup { id: number; name: string; }
export interface Category { id: number; name: string; groups: CatGroup[]; }
export interface CatalogBundle { categories: Category[]; all_group_ids: number[]; }
export interface GroupEntry { id: number; name: string; category_id: number; category_name: string; }

// Bundle ∪ additions as a category tree: each addition slotted under its category
// (creating the category if new), skipping any group id the bundle already lists.
// Categories are sorted by name; groups within a category by name.
export function mergeCatalog(bundle: CatalogBundle, additions: GroupEntry[]): Category[] {
  const cats = new Map<number, Category>();
  for (const c of bundle.categories) {
    cats.set(c.id, { id: c.id, name: c.name, groups: [...c.groups] });
  }
  const known = new Set(bundle.all_group_ids);
  for (const a of additions) {
    if (known.has(a.id)) continue;
    let cat = cats.get(a.category_id);
    if (!cat) {
      cat = { id: a.category_id, name: a.category_name, groups: [] };
      cats.set(a.category_id, cat);
    }
    if (!cat.groups.some((g) => g.id === a.id)) cat.groups.push({ id: a.id, name: a.name });
  }
  const out = [...cats.values()];
  for (const c of out) c.groups.sort((x, y) => x.name.localeCompare(y.name));
  out.sort((x, y) => x.name.localeCompare(y.name));
  return out;
}

// The tree narrowed to a case-insensitive name query. A category whose own name
// matches keeps all its groups; otherwise only its matching groups are kept.
// Categories left with no groups are dropped. Empty query returns the tree as-is.
export function filterCatalog(cats: Category[], query: string): Category[] {
  const q = query.trim().toLowerCase();
  if (!q) return cats;
  const out: Category[] = [];
  for (const c of cats) {
    if (c.name.toLowerCase().includes(q)) { out.push(c); continue; }
    const groups = c.groups.filter((g) => g.name.toLowerCase().includes(q));
    if (groups.length) out.push({ ...c, groups });
  }
  return out;
}

// Add or remove a group id from a membership list, returning a new sorted array.
export function toggleGroup(groups: number[], id: number, on: boolean): number[] {
  const set = new Set(groups);
  if (on) set.add(id); else set.delete(id);
  return [...set].sort((a, b) => a - b);
}

// Preset group ids not present anywhere in the catalog (shown as `#id`, removable).
export function unknownGroups(cats: Category[], presetGroups: number[]): number[] {
  const known = new Set<number>();
  for (const c of cats) for (const g of c.groups) known.add(g.id);
  return presetGroups.filter((id) => !known.has(id));
}
