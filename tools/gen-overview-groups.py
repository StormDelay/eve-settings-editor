#!/usr/bin/env python3
"""Regenerate app/src/lib/data/overview-groups.json.

The overview filter presets store entity *group IDs*. This dev tool builds the
bundled catalog the app uses to name and browse those groups: the overview-
relevant category -> group tree (with names) plus a flat list of ALL current
group IDs (the sync-diff baseline; the app resolves anything newer from ESI).

Relevance is a hardcoded allowlist of "in-space" category IDs — the categories
whose items appear on the overview. Tune RELEVANT_CATEGORIES if CCP adds a new
in-space category. A category id that no longer resolves (renamed/removed) is
skipped with a warning rather than failing the whole run.

Not shipped to users. Rerun and re-commit on an app release when CCP adds groups.

Usage:  python tools/gen-overview-groups.py
Requires Python 3 (stdlib only) and network access to ESI.
"""
import json
import os
import sys
import urllib.error
import urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "app", "src", "lib", "data", "overview-groups.json")
BASE = "https://esi.evetech.net/latest"

# In-space categories shown on the overview. IDs are stable across EVE versions.
# The labels here are only for the skip-warning; the bundle uses ESI's own name.
RELEVANT_CATEGORIES = {
    2: "Celestial",
    3: "Station",
    6: "Ship",
    11: "Entity",
    18: "Drone",
    22: "Deployable",
    23: "Starbase",
    25: "Asteroid",
    40: "Sovereignty Structures",
    46: "Orbitals",
    65: "Structure",
    87: "Fighter",
}


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "eve-settings-editor-gen"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.load(resp)


def all_group_ids():
    """Every current group id, following ESI's X-Pages pagination."""
    ids, page = [], 1
    while True:
        req = urllib.request.Request(f"{BASE}/universe/groups/?page={page}",
                                     headers={"User-Agent": "eve-settings-editor-gen"})
        with urllib.request.urlopen(req, timeout=30) as resp:
            ids.extend(json.load(resp))
            pages = int(resp.headers.get("X-Pages", "1"))
        if page >= pages:
            return ids
        page += 1


def main():
    categories = []
    for cat_id in sorted(RELEVANT_CATEGORIES):
        try:
            cat = get(f"{BASE}/universe/categories/{cat_id}/")
        except urllib.error.HTTPError as e:
            print(f"warning: category {cat_id} ({RELEVANT_CATEGORIES[cat_id]}) failed: "
                  f"{e.code} {e.reason} — skipping", file=sys.stderr)
            continue
        groups = []
        for gid in cat.get("groups", []):
            g = get(f"{BASE}/universe/groups/{gid}/")
            # NOT filtered by `published`: the core in-space overview entries —
            # Stargate, Station, Planet, Moon, Asteroid Belt, Wreck — are all
            # `published: false` in ESI yet appear on every overview. Filtering
            # them out left the catalog unable to name them. Any named group in
            # an in-space category is includable; only nameless internals are cut.
            name = g.get("name")
            if not name:
                continue
            groups.append({"id": gid, "name": name})
        groups.sort(key=lambda x: x["name"].lower())
        categories.append({"id": cat_id, "name": cat["name"], "groups": groups})
        print(f"  category {cat_id}: {cat['name']!r} -> {len(groups)} groups")

    ids = sorted(set(all_group_ids()))

    # Refuse to clobber the committed snapshot with an empty/degenerate result.
    if not ids or not any(c["groups"] for c in categories):
        sys.exit("ESI returned no groups — refusing to overwrite the snapshot")

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        json.dump({"categories": categories, "all_group_ids": ids}, fh, ensure_ascii=False, indent=2)
        fh.write("\n")

    total = sum(len(c["groups"]) for c in categories)
    print(f"wrote {len(categories)} categories, {total} groups, {len(ids)} total ids -> {OUT}")


if __name__ == "__main__":
    main()
