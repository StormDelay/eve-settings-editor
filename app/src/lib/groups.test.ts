// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts, overview.test.ts and search.test.ts.
import { mergeCatalog, filterCatalog, toggleGroup, unknownGroups, type CatalogBundle } from "./groups.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};
const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

const bundle: CatalogBundle = {
  categories: [
    { id: 6, name: "Ship", groups: [{ id: 25, name: "Frigate" }, { id: 26, name: "Cruiser" }] },
    { id: 18, name: "Drone", groups: [{ id: 100, name: "Combat Drone" }] },
  ],
  all_group_ids: [25, 26, 100],
};

{
  const cats = mergeCatalog(bundle, [{ id: 27, name: "Battleship", category_id: 6, category_name: "Ship" }]);
  const ship = cats.find((c) => c.id === 6)!;
  check(
    "mergeCatalog slots an addition under its existing category, sorted",
    eq(ship.groups.map((g) => g.name), ["Battleship", "Cruiser", "Frigate"]),
  );
}

{
  const cats = mergeCatalog(bundle, [{ id: 200, name: "Fighter", category_id: 87, category_name: "Fighter" }]);
  check(
    "mergeCatalog creates a category for an addition in a new category",
    !!cats.find((c) => c.id === 87 && c.groups.some((g) => g.id === 200)),
  );
}

{
  const cats = mergeCatalog(bundle, [{ id: 25, name: "Frigate", category_id: 6, category_name: "Ship" }]);
  const ship = cats.find((c) => c.id === 6)!;
  check(
    "mergeCatalog ignores an addition already present in the bundle",
    ship.groups.filter((g) => g.id === 25).length === 1,
  );
}

{
  const cats = filterCatalog(mergeCatalog(bundle, []), "frig");
  check("filterCatalog matches group names and drops empty categories", eq(cats.map((c) => c.name), ["Ship"]));
  check("filterCatalog keeps only the matching groups", eq(cats[0].groups.map((g) => g.name), ["Frigate"]));
}

{
  const cats = filterCatalog(mergeCatalog(bundle, []), "drone");
  check(
    "filterCatalog keeps all groups when the category name matches",
    cats.find((c) => c.name === "Drone")!.groups.length === 1,
  );
}

check("toggleGroup adds, returning a sorted array", eq(toggleGroup([26, 25], 100, true), [25, 26, 100]));
check("toggleGroup removes", eq(toggleGroup([25, 26], 25, false), [26]));

{
  const cats = mergeCatalog(bundle, []);
  check("unknownGroups returns preset IDs not in any category", eq(unknownGroups(cats, [25, 999]), [999]));
}

console.log("groups: all checks passed");
