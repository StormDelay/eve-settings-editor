#!/usr/bin/env python3
"""Regenerate app/src/lib/data/default-presets.json.

EVE's built-in overview profiles live in the client's static data, not the
settings file, so a clean account stores none of them. This tool bundles their
definitions (groups + state lists) so the app can list, show, and fork them.

The blobs come from the `overview_dump` bin in BLOBS mode over the local corpus
(TSV: BLOB<TAB>key<TAB>groups_csv<TAB>filtered_csv<TAB>alwaysshown_csv); the
richest copy (most groups) per key wins. Names: DefaultPreset_<id> via
default-preset-names.json; legacy default* via LEGACY_NAMES.

Usage:  find testdata -name 'core_user*.dat' | BLOBS=1 \\
            target/debug/overview_dump.exe | python tools/gen-default-presets.py
Requires Python 3 (stdlib only) and the built overview_dump bin.
"""
import json, os, re, sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "app", "src", "lib", "data", "default-presets.json")
NAMES = json.load(open(os.path.join(REPO, "app", "src", "lib", "data", "default-preset-names.json"), encoding="utf-8"))
LEGACY_NAMES = {"defaultall": "All", "defaultpvp": "PvP", "defaultmining": "Mining",
                "defaultloot": "Loot", "defaultdrones": "Drones", "defaultwarpto": "Warp To",
                "default": "Default"}

def ints(csv):
    return [int(x) for x in csv.split(",") if x]

def main():
    best = {}  # key -> (groups, filtered, always)
    for line in sys.stdin:
        parts = line.rstrip("\n").split("\t")
        if len(parts) != 5 or parts[0] != "BLOB":
            continue
        _, key, g, f, a = parts
        g, f, a = ints(g), ints(f), ints(a)
        if key not in best or len(g) > len(best[key][0]):
            best[key] = (g, f, a)

    modern, legacy = [], []
    for key, (g, f, a) in sorted(best.items()):
        m = re.match(r"^DefaultPreset_(\d+)$", key)
        if m:
            name = NAMES.get(m.group(1), key)
            modern.append({"key": key, "name": name, "groups": sorted(g),
                           "filteredStates": f, "alwaysShownStates": a})
        else:
            name = LEGACY_NAMES.get(key, key)
            legacy.append({"key": key, "name": name, "groups": sorted(g),
                           "filteredStates": f, "alwaysShownStates": a})

    if not modern and not legacy:
        sys.exit("no default profiles found on stdin — refusing to overwrite the snapshot")

    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        json.dump({"modern": modern, "legacy": legacy}, fh, ensure_ascii=False, indent=2)
        fh.write("\n")
    print(f"wrote {len(modern)} modern + {len(legacy)} legacy default profiles -> {OUT}")

if __name__ == "__main__":
    main()
