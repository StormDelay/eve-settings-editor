#!/usr/bin/env python3
"""Regenerate app/src/lib/data/command-names.json.

DO NOT RE-RUN BLINDLY — the committed JSON is HAND-CORRECTED. 84 of the 101
known commands resolve to EVE's own in-game strings via the SharedCache
localization pickle (FullPath "UI/Commands" and
"UI/Fleet/FleetBroadcast/Commands"); the remaining 17 fall back to a
de-camelcased name, two of which read badly and were fixed by hand:
  CmdPickPortrait0..3            -> "Pick Portrait 0".."Pick Portrait 3"
  ToggleCurrentSystemLocationWnd -> "Toggle Current System Location Window"
Re-verify those after regenerating.

Groups come from the command-name prefix families (see
docs/settings-field-reference.md §5.3); they are ours, not CCP's.

Not shipped to app users — reads the local EVE install. Rerun after an EVE
update that adds commands.

Usage:
    python tools/gen-command-names.py            # auto-discover
    python tools/gen-command-names.py --pickle <main> --en <en-us>

Requires Python 3 (stdlib only) and a local EVE install.
"""
import argparse
import json
import os
import pickle
import re
import sys

OUT = os.path.join("app", "src", "lib", "data", "command-names.json")

# Ordered: first match wins. Patterns are matched against the raw command name.
GROUPS = [
    (r"^CmdOverload", "Overload"),
    (r"^CmdActivate(High|Medium|Low)PowerSlot", "Modules"),
    (r"^Cmd(Drones|LaunchFavoriteDrones|ReconnectToDrones|SelectAllFighters)", "Drones & Fighters"),
    (r"^CmdFleetBroadcast|^CmdSendBroadcast", "Fleet broadcasts"),
    (r"^Cmd(Approach|KeepItemAtRange|WarpTo|AlignTo|DockOrJump|ToggleAutopilot|Accelerate|Decelerate|SetShipFullSpeed|StopShip|FlightControls)", "Navigation"),
    (r"^Cmd(LockTarget|UnlockTarget|SelectNextTarget|SelectPrevTarget|ToggleShipSelection|ToggleLookAtItem)", "Targeting"),
    (r"^(Open|Toggle)", "Windows"),
]
DEFAULT_GROUP = "Misc"


# CORRECTED (the plan's original version called
# gen-default-preset-names.py's find_localization_pickle() with a `name=`
# kwarg it does not accept — that function takes no arguments and hardcodes
# the en-US pickle. The two pickles also have unrelated content-hashed
# filenames, so deriving one from the other by string surgery cannot work.
# Scan the index once for both.)
def find_localization_pickles(args):
    """Locate the main + en-US localization pickles via a SharedCache resfileindex."""
    if args.pickle and args.en:
        return args.pickle, args.en

    # Reuse the drive discovery from the sibling tool rather than duplicating it.
    from importlib.machinery import SourceFileLoader
    helper = SourceFileLoader(
        "genpresets", os.path.join(os.path.dirname(__file__), "gen-default-preset-names.py")
    ).load_module()

    wanted = {
        "res:/localizationfsd/localization_fsd_main.pickle": None,
        "res:/localizationfsd/localization_fsd_en-us.pickle": None,
    }
    for drive in helper.fixed_drive_roots():
        for cand in ("SharedCache", os.path.join("EVE Shared Cache", "SharedCache")):
            idx = os.path.join(drive, cand, "tq", "resfileindex.txt")
            if not os.path.isfile(idx):
                continue
            with open(idx, encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    for res in wanted:
                        if wanted[res] is None and line.startswith(res + ","):
                            cache_rel = line.split(",", 2)[1]
                            p = os.path.join(drive, cand, "ResFiles",
                                             cache_rel.replace("/", os.sep))
                            if os.path.isfile(p):
                                wanted[res] = p
            if all(wanted.values()):
                break
        if all(wanted.values()):
            break

    return (wanted["res:/localizationfsd/localization_fsd_main.pickle"],
            wanted["res:/localizationfsd/localization_fsd_en-us.pickle"])


def decamel(name):
    n = re.sub(r"^Cmd", "", name)
    n = n.replace("_", ": ")
    n = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", n)
    return n.strip()


def group_for(name):
    for pattern, group in GROUPS:
        if re.search(pattern, name):
            return group
    return DEFAULT_GROUP


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pickle", help="localization_fsd_main.pickle")
    ap.add_argument("--en", help="localization_fsd_en-us.pickle")
    ap.add_argument("--commands", help="newline-separated command names; default: the keys already in the JSON")
    args = ap.parse_args()

    main_p, en_p = find_localization_pickles(args)
    if not main_p or not en_p:
        sys.exit("could not find the localization pickles; pass --pickle and --en")

    labels = pickle.load(open(main_p, "rb"), encoding="latin-1")["labels"]
    en = pickle.load(open(en_p, "rb"), encoding="latin-1")[1]

    byname = {}
    for mid, v in labels.items():
        byname.setdefault(v["label"], []).append((v["FullPath"], mid))

    if args.commands:
        names = [l.strip() for l in open(args.commands) if l.strip()]
    else:
        names = sorted(json.load(open(OUT)).keys())

    out, resolved = {}, 0
    for name in names:
        cands = byname.get(name, []) or byname.get(name.split("_")[-1], [])
        pick = next((c for c in cands if "Commands" in c[0]), cands[0] if cands else None)
        text = en.get(pick[1])[0] if pick and en.get(pick[1]) else None
        if text:
            resolved += 1
        out[name] = {"label": text or decamel(name), "group": group_for(name)}

    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(out, f, indent=2, sort_keys=True, ensure_ascii=False)
        f.write("\n")
    print(f"wrote {OUT}: {len(out)} commands, {resolved} from the client, {len(out)-resolved} de-camelcased")


if __name__ == "__main__":
    main()
