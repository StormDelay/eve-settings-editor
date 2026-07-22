#!/usr/bin/env python3
"""Regenerate app/src/lib/data/overview-groups.json.

The overview filter presets store entity *group IDs*. This dev tool builds the
bundled catalog the app uses to name and browse those groups: the overview
category -> group tree (with names) plus a flat list of ALL current group IDs
(the sync-diff baseline; the app resolves anything newer from ESI).

WHICH groups appear on the overview is a hand-curated EVE client definition that
is NOT derivable from any ESI flag (`published`, type-presence, brackets, and the
SDE all fail to reproduce the in-game Types tab — see the slice-2b investigation).
So the group SET comes from EVE's own data: `tools/overview-group-ids.json`, the
union of the `groups` lists across the real Tranquility settings files in
`testdata/corpus` (which includes CCP's built-in "All" preset plus the groups real
players actually put on overviews). Regenerate that id file with the
`overview_dump` bin (see crates/settings-model/src/bin/overview_dump.rs):
    find testdata -name 'core_user*.dat' | grep tq_tranquility \\
        | cargo run -q -p settings-model --bin overview_dump
    # take the UNION_IDS=[...] line -> tools/overview-group-ids.json

This tool then resolves each id's name + category from ESI and groups them. A few
corpus ids no longer resolve on TQ (removed groups) and are skipped.

Not shipped to users. Rerun and re-commit on an app release, refreshing the id
file first if the corpus has grown.

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
GROUP_IDS = os.path.join(REPO, "tools", "overview-group-ids.json")
BASE = "https://esi.evetech.net/latest"


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
    with open(GROUP_IDS, encoding="utf-8") as fh:
        overview_ids = json.load(fh)

    cat_names = {}      # category id -> name
    by_cat = {}         # category id -> [ {id, name} ]
    skipped = []
    for gid in overview_ids:
        try:
            g = get(f"{BASE}/universe/groups/{gid}/")
        except urllib.error.HTTPError:
            skipped.append(gid)  # removed from TQ since the corpus was captured
            continue
        name = g.get("name")
        if not name:
            continue
        cid = g["category_id"]
        if cid not in cat_names:
            cat_names[cid] = get(f"{BASE}/universe/categories/{cid}/").get("name", str(cid))
        by_cat.setdefault(cid, []).append({"id": gid, "name": name})

    categories = []
    for cid in sorted(by_cat, key=lambda c: cat_names[c].lower()):
        groups = sorted(by_cat[cid], key=lambda x: x["name"].lower())
        categories.append({"id": cid, "name": cat_names[cid], "groups": groups})

    ids = sorted(set(all_group_ids()))

    # Refuse to clobber the committed snapshot with an empty/degenerate result.
    if not ids or not any(c["groups"] for c in categories):
        sys.exit("ESI returned no groups — refusing to overwrite the snapshot")

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        json.dump({"categories": categories, "all_group_ids": ids}, fh, ensure_ascii=False, indent=2)
        fh.write("\n")

    total = sum(len(c["groups"]) for c in categories)
    for c in categories:
        print(f"  {c['name']}: {len(c['groups'])} groups")
    print(f"wrote {len(categories)} categories, {total} groups, {len(ids)} total ids "
          f"({len(skipped)} corpus ids skipped as removed-from-TQ) -> {OUT}")


if __name__ == "__main__":
    main()
