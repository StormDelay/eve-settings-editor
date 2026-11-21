#!/usr/bin/env python3
"""Regenerate the client's per-window size limits from the EVE client's code:

  crates/settings-model/data/window-sizes.json   window id -> min/fixed size

The layout rescale needs them: the client's `Window.InitializeSize` raises a
saved size to the window's minimum and replaces a fixed dimension outright, so
a rescale that ignored them would show a canvas the game does not reproduce.

They are class constants. `carbonui/control/window.py` declares
`default_minSize = (default_width, default_height)` (256x128, evaluated once in
the base class body) plus `default_fixedWidth` / `default_fixedHeight`, and
every window class overrides what it needs next to its `default_windowID`.
This tool evaluates every class body statically: constants, tuples, simple
arithmetic, module-level constants and imported constant modules
(`overviewConst.WINDOW_ID`). Inheritance follows Python: a child's class
attributes override its base's, and a name inside a class body sees the body
and the module, never the base class. A class whose id or minimum does not
reduce to constants is skipped and printed rather than guessed.

About 66 classes also call `SetMinSize` at runtime; for them the table is the
static floor. Sizes are in UI-scaled px, the same unit as the saved geometry.

Usage: python -I tools/gen-window-sizes.py [path/to/code.ccp]
       python -I tools/gen-window-sizes.py --check [path]   the committed table is this client's
       python -I tools/gen-window-sizes.py --self-test
Requires Python 3 (stdlib only) and a local EVE install.
"""
import importlib.util
import json
import os
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "crates", "settings-model", "data", "window-sizes.json")

_spec = importlib.util.spec_from_file_location("gen_commands", os.path.join(HERE, "gen-commands.py"))
gc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gc)

WINDOW = ("carbonui.control.window", "Window")
KEYS = ("default_windowID", "default_minSize", "default_fixedWidth", "default_fixedHeight")
# Python 2.7 opcodes the evaluator models; anything else clears the stack.
NAME_OPS = {90: "STORE_NAME", 101: "LOAD_NAME", 106: "LOAD_ATTR", 108: "IMPORT_NAME",
            109: "IMPORT_FROM", 116: "LOAD_GLOBAL"}
# Jumps and block setups: FOR_ITER, JUMP_*, POP_JUMP_IF_*, SETUP_LOOP/EXCEPT/
# FINALLY, SETUP_WITH.
JUMPS = {93, 110, 111, 112, 113, 114, 115, 120, 121, 122, 143}
BINARY = {20: lambda a, b: a * b, 23: lambda a, b: a + b, 24: lambda a, b: a - b,
          21: lambda a, b: a // b, 26: lambda a, b: a // b}
# Parameterized window ids that are not a class's `default_windowID`.
# `chatchannel_<key>` is every chat channel's window (docs/format-notes.md).
PREFIXES = {"chatchannel_": "ChatWindow"}


class Unknown:
    def __repr__(self):
        return "?"


UNK = Unknown()


class ModuleRef:
    def __init__(self, path):
        self.path = path


class Imported:
    def __init__(self, mod, name):
        self.mod, self.name = mod, name


class Func:
    def __init__(self, code):
        self.code = code


class Body:
    def __init__(self, code):
        self.code = code


class ClassRef:
    def __init__(self, mod, name, bases, body):
        self.mod, self.name, self.bases, self.body = mod, name, bases, body
        self._attrs = None


def ops(code):
    b, i = code.code.encode("latin-1"), 0
    while i < len(b):
        op, arg = b[i], None
        if op >= 90:
            arg, i = b[i + 1] | b[i + 2] << 8, i + 3
            if op == 100:
                arg = code.consts[arg]
            elif op in NAME_OPS:
                arg = code.names[arg]
        else:
            i += 1
        yield op, arg


class Client:
    def __init__(self, ccp):
        self.ccp = ccp
        self.members = set(ccp.namelist()) if ccp else set()
        self.modules = {}

    def module(self, path):
        """A module's top-level names, or None when it is not in code.ccp."""
        if path in self.modules:
            return self.modules[path]
        member = path.replace(".", "/") + ".pyj"
        if member not in self.members:
            self.modules[path] = None
            return None
        self.modules[path] = {}  # cycle guard: a re-entrant lookup sees nothing yet
        try:
            code = gc.load(self.ccp, member)
        except Exception:
            return {}
        env = self.run(code, path, {}, lambda n: UNK)
        self.modules[path] = env
        return env

    def deref(self, v, depth=0):
        while isinstance(v, Imported) and depth < 20:
            env = self.module(v.mod)
            if env is not None and v.name in env:
                v = env[v.name]
            elif self.module(v.mod + "." + v.name) is not None:
                return ModuleRef(v.mod + "." + v.name)
            else:
                return UNK
            depth += 1
        return v

    def attr(self, obj, name):
        obj = self.deref(obj)
        if isinstance(obj, ModuleRef):
            env = self.module(obj.path)
            if env is not None and name in env:
                return self.deref(env[name])
            if self.module(obj.path + "." + name) is not None:
                return ModuleRef(obj.path + "." + name)
            if obj.path.rsplit(".", 1)[-1] == name:  # `import a.b as c` -> LOAD_ATTR b
                return obj
        return UNK

    def run(self, code, mod, env, outer):
        """Evaluate a module or class body straight through, ignoring jumps.

        Straight through is only right until the first branch: past one, which
        assignment wins depends on a condition the tool cannot evaluate, so a
        size key stored after it is unknown rather than its last value."""
        st, branched = [], False
        pop = lambda: st.pop() if st else UNK
        for op, arg in ops(code):
            if op == 100:
                st.append(arg)
            elif op in (101, 116):
                st.append(env[arg] if arg in env else outer(arg))
            elif op == 106:
                st.append(self.attr(pop(), arg))
            elif op == 108:  # IMPORT_NAME: pops level and fromlist
                pop(), pop()
                st.append(ModuleRef(arg))
            elif op == 109:  # IMPORT_FROM leaves the module on the stack
                m = st[-1] if st else UNK
                st.append(Imported(m.path, arg) if isinstance(m, ModuleRef) else UNK)
            elif op == 102:  # BUILD_TUPLE
                n = arg
                vals = [self.deref(v) for v in st[len(st) - n:]] if n else []
                del st[len(st) - n:]
                st.append(tuple(vals))
            elif op in BINARY:
                r, l = self.deref(pop()), self.deref(pop())
                ok = all(isinstance(x, int) and not isinstance(x, bool) for x in (l, r))
                st.append(BINARY[op](l, r) if ok and not (op in (21, 26) and r == 0) else UNK)
            elif op == 11:  # UNARY_NEGATIVE
                v = self.deref(pop())
                st.append(-v if isinstance(v, int) else UNK)
            elif op in (132, 134):  # MAKE_FUNCTION / MAKE_CLOSURE
                body = pop()
                if op == 134:
                    pop()
                for _ in range(arg):
                    pop()
                st.append(Func(body))
            elif op == 131:  # CALL_FUNCTION
                n = (arg & 0xFF) + 2 * (arg >> 8)
                args = [pop() for _ in range(n)]
                fn = pop()
                st.append(Body(fn.code) if isinstance(fn, Func) and not args else UNK)
            elif op == 89:  # BUILD_CLASS
                body, bases, name = pop(), pop(), pop()
                if isinstance(body, Body) and isinstance(name, str):
                    bases = tuple(bases) if isinstance(bases, (tuple, list)) else ()
                    st.append(ClassRef(mod, name, bases, body.code))
                else:
                    st.append(UNK)
            elif op in JUMPS:
                branched = True
                st.clear()
            elif op == 90:
                v = pop()
                env[arg] = UNK if branched and arg in KEYS else v
            elif op == 1:
                pop()
            elif op == 83:
                break
            else:
                st.clear()
        return env

    def attrs(self, cls):
        """The class's own KEYS over its bases', Python's override order."""
        if cls._attrs is not None:
            return cls._attrs
        cls._attrs = {}  # cycle guard
        merged = {}
        for base in reversed(cls.bases):
            base = self.deref(base)
            if isinstance(base, ClassRef):
                merged.update(self.attrs(base))
        menv = self.module(cls.mod) or {}
        body = self.run(cls.body, cls.mod, {}, lambda n: self.deref(menv.get(n, UNK)))
        merged.update({k: self.deref(body[k]) for k in KEYS if k in body})
        cls._attrs = merged
        return merged

    def is_window(self, cls, depth=0):
        if (cls.mod, cls.name) == WINDOW:
            return True
        return depth < 40 and any(
            isinstance(b, ClassRef) and self.is_window(b, depth + 1)
            for b in (self.deref(b) for b in cls.bases))


def size(v):
    return v if isinstance(v, int) and not isinstance(v, bool) and v > 0 else None


def build(ccp_path):
    """The table, and the classes skipped, as this client's code gives them."""
    client = Client(zipfile.ZipFile(ccp_path))
    classes = []
    for member in sorted(client.members):
        if member.endswith(".pyj"):
            env = client.module(member[:-4].replace("/", "."))
            classes += [v for v in (env or {}).values() if isinstance(v, ClassRef)]

    ids, skipped = {}, []
    for cls in classes:
        if not client.is_window(cls):
            continue
        a = client.attrs(cls)
        wid, mins = a.get("default_windowID"), a.get("default_minSize")
        label = f"{cls.mod}.{cls.name}"
        if not isinstance(wid, str):
            if wid is not None:
                skipped.append(f"{label}: window id {wid!r}")
            continue
        if not (isinstance(mins, (tuple, list)) and len(mins) == 2 and all(isinstance(m, int) for m in mins)):
            skipped.append(f"{label} ({wid}): minimum {mins!r}")
            continue
        entry = {"min_w": max(0, mins[0]), "min_h": max(0, mins[1])}
        for key, field in (("default_fixedWidth", "fixed_w"), ("default_fixedHeight", "fixed_h")):
            v = a.get(key)
            if v is UNK:
                skipped.append(f"{label} ({wid}): {key} not constant")
                entry = None
                break
            if size(v):
                entry[field] = v
        if entry is None:
            continue
        prev = ids.get(wid)
        if prev and prev != entry:
            # Two classes share an id (Selected Item has two): keep the smaller
            # floor, the one that never raises a window the game would not.
            print(f"conflict {wid}: {prev} vs {entry} ({label})")
            entry = {"min_w": min(prev["min_w"], entry["min_w"]), "min_h": min(prev["min_h"], entry["min_h"]),
                     **{k: prev[k] for k in ("fixed_w", "fixed_h") if k in prev and prev.get(k) == entry.get(k)}}
        ids[wid] = entry

    prefixes = {p: ids[cls_id] for p, cls_id in PREFIXES.items() if cls_id in ids}
    for p, cls_id in PREFIXES.items():
        if cls_id not in ids:
            print(f"prefix {p}: no limits for {cls_id}")
    out = {"_source": "generated by tools/gen-window-sizes.py from code.ccp",
           "ids": dict(sorted(ids.items())), "prefixes": prefixes}
    return out, skipped


def self_test():
    """A size key assigned inside a branch is not a constant: the evaluator runs
    straight through and would otherwise keep whichever branch came last."""
    def code(ops_bytes, consts, names):
        c = gc.Code()
        c.code, c.consts, c.names = bytes(ops_bytes).decode("latin-1"), consts, names
        return c
    # default_minSize = (300, 200)
    straight = code([100, 0, 0, 90, 0, 0, 82, 83], [[300, 200]], ["default_minSize"])
    # if flag: default_minSize = (300, 200)
    branched = code([100, 0, 0, 114, 12, 0, 100, 1, 0, 90, 0, 0, 82, 83], [True, [300, 200]], ["default_minSize"])
    client = Client(None)
    assert client.run(straight, "m", {}, lambda n: UNK)["default_minSize"] == [300, 200]
    assert client.run(branched, "m", {}, lambda n: UNK)["default_minSize"] is UNK, "a branch must make the key unknown"
    print("self-test ok")


def main():
    args = sys.argv[1:]
    if args == ["--self-test"]:
        return self_test()
    check = "--check" in args
    args = [a for a in args if a != "--check"]
    ccp_path = args[0] if args else gc.find_ccp()
    if not ccp_path:
        if check:
            print("no code.ccp here - skipping the check")
            return
        sys.exit("no code.ccp found; pass its path")
    out, skipped = build(ccp_path)
    if check:
        # The committed table must be exactly what this client's code gives.
        with open(OUT, encoding="utf-8") as f:
            committed = json.load(f)
        if committed != out:
            sys.exit(f"{os.path.normpath(OUT)} is stale: run tools/gen-window-sizes.py")
        print(f"{os.path.normpath(OUT)} matches this client ({len(out['ids'])} ids)")
        return
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        json.dump(out, f, indent=1, sort_keys=False)
        f.write("\n")
    print(f"{len(out['ids'])} window ids written to {os.path.normpath(OUT)}; {len(skipped)} skipped:")
    for line in skipped:
        print("  " + line)


if __name__ == "__main__":
    main()
