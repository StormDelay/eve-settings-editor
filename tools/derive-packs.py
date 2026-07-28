# Two overview-pack variants the live verification plan needs and no published
# pack provides:
#   item 24 - a tab-layout-only pack (no `presets` section)
#   item 25 - a pack with MORE tabs than the account under test
#
#   python tools/derive-packs.py [pack directory]
#
# Both are text edits, not a YAML round trip: a pack is flat at the top level,
# so a section is "from `<name>:` to the next line starting in column 0", and
# editing it as text leaves every byte we are not deleting exactly as its author
# wrote it. Round-tripping through a YAML library would silently renormalise
# quoting and ordering, which is the one thing these files must not do — the
# whole point of a community pack is that it carries shapes we did not write.
import os
import re
import sys

D = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser(r"~\Documents\EVE\Overview")


def split_sections(path):
    lines = open(path, encoding="utf-8").read().splitlines(keepends=True)
    bounds = [i for i, l in enumerate(lines) if re.match(r"^[a-zA-Z]", l)] + [len(lines)]
    return lines, {lines[bounds[i]].split(":")[0]: (bounds[i], bounds[i + 1])
                   for i in range(len(bounds) - 1)}


# item 24: Fenris minus its presets. Its tabs keep naming presets that are no
# longer in the file, which is the second untested path — tabs whose
# overview/bracket names the account has no preset for.
lines, sec = split_sections(os.path.join(D, "fenris_default_v24.01.yaml"))
a, b = sec["presets"]
out = lines[:a] + lines[b:]
open(os.path.join(D, "derived_tabs_only_no_presets.yaml"), "w", encoding="utf-8").writelines(out)
print(f"tab-layout-only: dropped presets lines {a + 1}-{b}, {len(out)} lines left")

# item 25: Z-S with more tabs than the account's 8.
lines, sec = split_sections(os.path.join(D, "zs_full_v10.06.09.yaml"))
a, b = sec["tabSetup"]
body = lines[a + 1:b]
# A tab entry starts at the one indent level that carries a bare integer index.
starts = [i for i, l in enumerate(body) if re.match(r"^\s+- - \d+\s*$", l)]
entries = [body[starts[i]:starts[i + 1] if i + 1 < len(starts) else len(body)]
           for i in range(len(starts))]
extra = []
for n in range(len(entries), len(entries) + 4):
    clone = list(entries[0])
    clone[0] = re.sub(r"\d+", str(n), clone[0])
    # Rename the clone so a duplicated tab is identifiable on sight in-game.
    clone = [re.sub(r"^(\s+- )(<b>.*</b>)$", rf"\g<1>dup{n}", l) for l in clone]
    extra += clone
out = lines[:b] + extra + lines[b:]
open(os.path.join(D, "derived_zs_more_tabs.yaml"), "w", encoding="utf-8").writelines(out)
print(f"more-tabs: {len(entries)} tabs -> {len(entries) + 4}")

# --- palette probe: move `black` onto a BACKGROUND state -------------------
# PALETTE (overview_pack.rs) holds 5 of the 8 names the client uses, and it can
# only grow by joining `restoreData->data`'s stateColorsNameList against the
# RGBA EVE derived into `stateColors` — both written by the client, never by us.
# Session A fed Z-S through EVE's own importer to harvest its `black`, and got
# nothing: Z-S sets `flag_48: black`, and **EVE discards flag-surface colours
# outright** — after the import `flag_48` was absent from the file entirely and
# `stateColors` held only `background` entries.
#
# So the probe has to put an unknown name on a surface the client keeps. State
# 66 ("Pilot has retribution timer") is the one whose built-in colour is black
# (overview-states.json defaultColors), which makes `background_66: black` the
# natural pairing rather than an arbitrary one.
lines, sec = split_sections(os.path.join(D, "zs_full_v10.06.09.yaml"))
a, b = sec["stateColorsNameList"]
body = lines[a + 1:b]
kept = [l for l in body if "flag_" not in l and l.strip() != "- black"]
indent = re.match(r"^(\s*)", kept[0]).group(1) if kept else "  "
probe = [f"{indent}- - background_66\n", f"{indent}  - black\n"]
out = lines[:a + 1] + kept + probe + lines[b:]
open(os.path.join(D, "derived_palette_probe.yaml"), "w", encoding="utf-8").writelines(out)
print("palette-probe: flag_48 dropped, background_66 -> black added")
