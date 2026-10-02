#!/usr/bin/env python3
"""Refuse a commit that carries an identifier from the owner's real EVE data.

The denylist is never written down: it is read at commit time from the real
settings files on this machine (the client's folders and the gitignored
testdata/ corpus) and the editor's name cache. Any number of 5+ digits, hex
run of 12+ chars or known character name in the staged additions or the commit
message that also occurs in that data blocks the commit. Public game data that
legitimately appears in both (CCP's own ids) is listed in public-ids.txt.

  real-ids.py MSGFILE   check the staged diff and the commit message (hook)
  real-ids.py --all     check every tracked file (audit)

Without any real data on the machine it does nothing.
"""
import functools, glob, hashlib, json, os, re, struct, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
TOKEN = re.compile(r"(?<![0-9a-fA-F.])[0-9a-f]{12,}(?![0-9a-fA-F])|(?<![0-9.])\d{5,}(?![0-9])")


def real_data():
    local = os.environ.get("LOCALAPPDATA", "")
    paths = glob.glob(os.path.join(local, "CCP", "EVE", "**", "core_*.dat"), recursive=True)
    paths += glob.glob(os.path.join(HERE, "..", "testdata", "**", "*.dat"), recursive=True)
    seen, blobs, ids = set(), [], set()
    for p in paths:
        m = re.match(r"core_(?:char|user)_(\d+)\.dat$", os.path.basename(p))
        if m: ids.add(m.group(1))
        with open(p, "rb") as f: b = f.read()
        h = hashlib.sha1(b).digest()
        if h not in seen: seen.add(h); blobs.append(b)
    names = set()
    appdata = os.environ.get("APPDATA", "")
    for p in glob.glob(os.path.join(appdata, "*", "names-cache.json")):
        try: cache = json.load(open(p, encoding="utf-8"))
        except (OSError, ValueError): continue
        for v in (cache.values() if isinstance(cache, dict) else []):
            n = v.get("name") if isinstance(v, dict) else v
            if isinstance(n, str) and len(n) >= 4: names.add(n)
    return b"\0".join(blobs), ids, names


@functools.cache
def is_real(tok):
    blob, ids, _ = DATA
    if tok in ids: return True
    b = tok.encode()
    i = blob.find(b)
    while i >= 0:  # standalone, not part of a longer digit/hex run
        before, after = blob[i - 1:i], blob[i + len(b):i + len(b) + 1]
        if not re.match(rb"[0-9a-f]", before) and not re.match(rb"[0-9a-f]", after): return True
        i = blob.find(b, i + 1)
    v = int(tok) if tok.isdigit() else -1
    return len(tok) >= 7 and 0 <= v < 2**31 and b"\x04" + struct.pack("<i", v) in blob  # marshal int32


def main():
    global DATA
    DATA = blob, ids, names = real_data()
    if not blob: return 0
    public = set()
    for l in open(os.path.join(HERE, "public-ids.txt"), encoding="utf-8"):
        lo, _, hi = l.split("#")[0].strip().partition("-")
        if lo: public |= {str(n) for n in range(int(lo), int(hi or lo) + 1)} if hi else {lo}
    git = lambda *a: subprocess.run(["git", *a], capture_output=True, text=True, encoding="utf-8", errors="replace").stdout
    if sys.argv[1:] == ["--all"]:
        texts = []
        for f in git("ls-files").splitlines():
            try: texts.append((f, open(f, encoding="utf-8").read()))
            except (OSError, UnicodeDecodeError): pass  # ponytail: binaries skipped, they come from checked sources
    else:
        diff = git("diff", "--cached", "-U0", "--no-color", "--diff-filter=d")
        texts, f = [], "?"
        for l in diff.splitlines():
            if l.startswith("+++ "): f = l[6:]
            elif l.startswith("+"): texts.append((f, l[1:]))
        texts.append(("commit message", open(sys.argv[1], encoding="utf-8").read()))
    hits = set()
    for f, t in texts:
        for m in TOKEN.finditer(t):
            if m.group() not in public and is_real(m.group()): hits.add((f, m.group()))
        hits |= {(f, n) for n in names if n in t and n != "StormDelay"}
    if hits:
        print("real-ids: identifiers from your real EVE data (use synthetic ones, see CLAUDE.md):", file=sys.stderr)
        for f, tok in sorted(hits): print(f"  {f}: {tok}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
