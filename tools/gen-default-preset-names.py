#!/usr/bin/env python3
"""Regenerate app/src/lib/data/default-preset-names.json.

EVE's built-in overview presets are stored under keys like ``DefaultPreset_639431``
where the number is a localization message id, not a readable name (the settings
file carries no label). This dev tool resolves those ids to their en-US labels
from the EVE client's SharedCache localization pickle and writes a small snapshot
the app bundles for display. The raw ``DefaultPreset_<id>`` key stays the source
of truth for edits; only the shown text uses this map.

Not shipped to app users — it reads the local EVE install. Rerun after an EVE
update if the default presets change (rare).

Usage:
    python tools/gen-default-preset-names.py            # auto-discover everything
    python tools/gen-default-preset-names.py --pickle <file> --settings <dir>

Requires Python 3 (stdlib only) and a local EVE install (for the localization
pickle). The pickle is Python pickle protocol 0; loaded read-only.
"""
import argparse
import glob
import json
import os
import pickle
import re
import string
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "app", "src", "lib", "data", "default-preset-names.json")
ID_RE = re.compile(rb"DefaultPreset_([0-9]+)")


def fixed_drive_roots():
    if os.name == "nt":
        return [f"{c}:\\" for c in string.ascii_uppercase if os.path.exists(f"{c}:\\")]
    return ["/"]


def find_localization_pickle():
    """Locate the en-US localization pickle via a SharedCache resfileindex."""
    for drive in fixed_drive_roots():
        for cand in ("SharedCache", os.path.join("EVE Shared Cache", "SharedCache")):
            idx = os.path.join(drive, cand, "tq", "resfileindex.txt")
            if not os.path.isfile(idx):
                continue
            with open(idx, encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    if line.startswith("res:/localizationfsd/localization_fsd_en-us.pickle"):
                        cache_rel = line.split(",", 2)[1]
                        p = os.path.join(drive, cand, "ResFiles", cache_rel.replace("/", os.sep))
                        if os.path.isfile(p):
                            return p
    return None


def find_settings_dirs():
    """Live EVE settings root(s) plus the repo corpus, for id discovery."""
    roots = []
    local = os.environ.get("LOCALAPPDATA")
    if local:
        roots.append(os.path.join(local, "CCP", "EVE"))
    corpus = os.path.join(REPO, "testdata", "corpus")
    if os.path.isdir(corpus):
        roots.append(corpus)
    return roots


def collect_ids(settings_roots):
    """Every DefaultPreset id in local core_user files (raw byte scan, no decode)."""
    ids = set()
    for root in settings_roots:
        for path in glob.glob(os.path.join(root, "**", "core_user_*.dat"), recursive=True):
            try:
                with open(path, "rb") as fh:
                    for m in ID_RE.finditer(fh.read()):
                        ids.add(int(m.group(1)))
            except OSError:
                pass
    return ids


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pickle", help="path to localization_fsd_en-us.pickle (auto-discovered if omitted)")
    ap.add_argument("--settings", action="append", help="EVE settings dir to scan for ids (repeatable)")
    ap.add_argument("--out", default=OUT, help=f"output JSON (default: {OUT})")
    args = ap.parse_args()

    pickle_path = args.pickle or find_localization_pickle()
    if not pickle_path or not os.path.isfile(pickle_path):
        sys.exit("could not find the en-US localization pickle; pass --pickle <file>")
    settings_roots = args.settings or find_settings_dirs()
    if not settings_roots:
        sys.exit("no settings dirs to scan; pass --settings <dir>")

    ids = collect_ids(settings_roots)
    if not ids:
        sys.exit("found no DefaultPreset ids in the scanned settings files")

    # protocol-0 pickle from an older client -> latin-1 for its byte strings.
    _lang, table = pickle.load(open(pickle_path, "rb"), encoding="latin-1")

    names, unresolved = {}, []
    for i in sorted(ids):
        entry = table.get(i)
        if entry and entry[0]:
            names[str(i)] = entry[0]
        else:
            unresolved.append(i)

    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(names, fh, ensure_ascii=False, indent=2, sort_keys=True)
        fh.write("\n")

    print(f"pickle:   {pickle_path}")
    print(f"scanned:  {', '.join(settings_roots)}")
    print(f"resolved: {len(names)} ids -> {args.out}")
    if unresolved:
        print(f"unresolved (left to fall back to the raw key): {unresolved}")


if __name__ == "__main__":
    main()
