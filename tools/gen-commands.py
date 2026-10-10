#!/usr/bin/env python3
"""Regenerate the keybinding editor's command data from the EVE client's code:

  app/src/lib/data/command-defaults.json  command -> factory keys, or null
  app/src/lib/data/command-names.json     command -> {label, group}

DEFAULTS. No settings file holds them (`customCmds` holds what the player
changed), but the client builds them in code: `CommandService.
SetDefaultShortcutMappingCORE` (carbonui/services/command.py) and
`EveCommandService.SetDefaultShortcutMappingGAME` (eve/client/script/ui/
eveCommands.py), which the client concatenates. This tool RUNS their bytecode
on a tiny stack machine, so the table comes out exactly as the client computes
it, loops and all. The functions branch on the environment; they are run as an
ordinary player on Tranquility under Windows (no developer role, not the
Chinese server, `sys.platform` not darwin, every feature flag on), and the
branches taken are printed so a changed one shows up. Locked mappings (Enter,
Esc, Tab, Ctrl+C, ... — `isLocked`, not rebindable) are left out; a command
EVE ships unbound is null, so the app can tell it from one the table lacks.
Modifiers are written in the order EVE stores them: Ctrl, Alt, Shift.

NAMES. The in-game keybinding screen labels a command the way
`CommandService.FuncToDesc` does: the module-level `labelsByFuncName` map
first, then the callback's `nameLabelPath` (set in the command class body,
often to a window class's `default_captionLabelPath`), else the raw name. Each
path is looked up in the SharedCache's en-US localization. The group is the
screen's tab: the mapping's `category`, named through eveCommands'
`CATEGORIES`. A command whose label cannot be resolved is written without one,
and the app de-camelcases its name.

`code.ccp` is a zip of `.pyj` files, each a zlib stream of a Python 2.7 `.pyc`
(8-byte header, then a marshal code object). Python 3 cannot unmarshal 2.7, so
the reader below does it by hand.

Not shipped to app users — reads the local EVE install. Rerun after an EVE
update.

Usage:
    python tools/gen-commands.py                 # auto-discover
    python tools/gen-commands.py --ccp <path/to/tq/code.ccp>

Requires Python 3 (stdlib only) and a local EVE install.
"""
import argparse
import json
import os
import pickle
import string
import struct
import sys
import zipfile
import zlib

DATA = os.path.join("app", "src", "lib", "data")
OUT_DEFAULTS = os.path.join(DATA, "command-defaults.json")
OUT_NAMES = os.path.join(DATA, "command-names.json")
CTRL, ALT, SHIFT = 17, 18, 16
MOD_ORDER = [CTRL, ALT, SHIFT]


# --- Python 2.7 marshal ------------------------------------------------------

class Code:
    pass


class Reader:
    def __init__(self, b):
        self.b, self.i, self.interned = b, 0, []

    def take(self, n):
        v = self.b[self.i:self.i + n]
        self.i += n
        return v

    def i32(self):
        return struct.unpack("<i", self.take(4))[0]

    def obj(self):
        t = chr(self.take(1)[0])
        if t in "0N":
            return None
        if t in "FT":
            return t == "T"
        if t == "i":
            return self.i32()
        if t == "I":
            return struct.unpack("<q", self.take(8))[0]
        if t == "l":
            n, v = self.i32(), 0
            for k in range(abs(n)):
                v |= struct.unpack("<H", self.take(2))[0] << (15 * k)
            return -v if n < 0 else v
        if t == "f":
            return float(self.take(self.take(1)[0]))
        if t == "g":
            return struct.unpack("<d", self.take(8))[0]
        if t in "st":
            v = self.take(self.i32()).decode("latin-1")
            if t == "t":
                self.interned.append(v)
            return v
        if t == "R":
            return self.interned[self.i32()]
        if t == "u":
            return self.take(self.i32()).decode("utf-8", "replace")
        if t in "([<>":
            return [self.obj() for _ in range(self.i32())]
        if t == "{":
            d = {}
            while self.b[self.i] != ord("0"):
                k = self.obj()
                d[k] = self.obj()
            self.i += 1
            return d
        if t == "c":
            c = Code()
            c.argcount, c.nlocals, c.stacksize, c.flags = (self.i32() for _ in range(4))
            c.code, c.consts, c.names, c.varnames = self.obj(), self.obj(), self.obj(), self.obj()
            c.freevars, c.cellvars, c.filename, c.name = self.obj(), self.obj(), self.obj(), self.obj()
            c.firstlineno, c.lnotab = self.i32(), self.obj()
            return c
        raise ValueError(f"unknown marshal type {t!r} at {self.i - 1}")


def load(ccp, module):
    data = zlib.decompress(ccp.read(module))
    return Reader(data[8:]).obj()


def child(code, name):
    return next(k for k in code.consts if isinstance(k, Code) and k.name == name)


# --- the stack machine -------------------------------------------------------

# Python 2.7 opcode numbers, only those the two functions use.
OP = dict(POP_TOP=1, BINARY_MODULO=22, INPLACE_ADD=55, BINARY_AND=64, GET_ITER=68,
          RETURN_VALUE=83, POP_BLOCK=87, STORE_NAME=90, FOR_ITER=93, STORE_ATTR=95,
          LOAD_CONST=100, BUILD_TUPLE=102, BUILD_LIST=103, LOAD_ATTR=106,
          JUMP_FORWARD=110, JUMP_ABSOLUTE=113, POP_JUMP_IF_FALSE=114,
          POP_JUMP_IF_TRUE=115, LOAD_GLOBAL=116, SETUP_LOOP=120, LOAD_FAST=124,
          STORE_FAST=125, CALL_FUNCTION=131, MAKE_FUNCTION=132)
NAME = {v: k for k, v in OP.items()}

# Environment calls that must come out false for an ordinary TQ player.
FALSE = ("AmOnChineseServer", "IsPlayerBountyHidden", "sys.platform")


class Sym:
    """A value from the environment the functions never get to see."""
    def __init__(self, name):
        self.name = name


class Method:
    """`self.CmdX`: a command callback, named the way the client names it."""
    def __init__(self, name):
        self.name = name

    def __call__(self, *a, **k):
        # MakeCmdTagItem("A") / MakeCmdTagItemFromSequence(["1", "2", "3"])
        # build a callback whose __name__ is "CmdTagItem_" + the tag(s).
        # Python 2's "%s" % None is "None", so MakeCmdTagItem(None) really is
        # CmdTagItem_None.
        if self.name == "MakeCmdTagItem":
            return Method("CmdTagItem_%s" % (a[0],))
        if self.name == "MakeCmdTagItemFromSequence":
            return Method("CmdTagItem_%s" % "".join(a[0]))
        return Sym(f"self.{self.name}()")


class Mapping:
    def __init__(self, callback, shortcut=None, **kw):
        self.callback, self.shortcut = callback, shortcut
        # CommandMapping.__init__: `self.category = category or "general"`.
        self.isLocked, self.category = kw.get("isLocked", False), kw.get("category") or "general"


class Obj:
    def __init__(self, attrs, fallback=None):
        self.attrs, self.fallback = attrs, fallback

    def __getattr__(self, n):
        if n in ("attrs", "fallback"):
            raise AttributeError(n)
        if n in self.attrs:
            return self.attrs[n]
        return self.fallback(n)


def module_ints(code):
    """`NAME = <int>` assignments at a module's top level, first one winning."""
    b, out, last, i = code.code.encode("latin-1"), {}, None, 0
    while i < len(b):
        op = b[i]
        if op >= 90:
            arg, i = b[i + 1] | b[i + 2] << 8, i + 3
            if op == OP["LOAD_CONST"]:
                last = code.consts[arg]
                continue
            if op == OP["STORE_NAME"] and type(last) is int:
                out.setdefault(code.names[arg], last)
        else:
            i += 1
        last = None
    return out


def run(code, args, globs, taken):
    b = code.code.encode("latin-1")
    local, stack, i = dict(zip(code.varnames, args)), [], 0
    while i < len(b):
        op, arg = b[i], None
        if op >= 90:
            arg, i = b[i + 1] | b[i + 2] << 8, i + 3
        else:
            i += 1
        n = NAME.get(op)
        if n == "LOAD_FAST":
            stack.append(local[code.varnames[arg]])
        elif n == "STORE_FAST":
            local[code.varnames[arg]] = stack.pop()
        elif n == "LOAD_CONST":
            v = code.consts[arg]
            # The nested `k(win, mac)` picks a per-OS shortcut: Windows.
            stack.append((lambda win=None, mac=None: win) if isinstance(v, Code) else v)
        elif n == "MAKE_FUNCTION":
            pass
        elif n == "LOAD_GLOBAL":
            g = code.names[arg]
            stack.append(globs[g] if g in globs else Sym(g))
        elif n == "LOAD_ATTR":
            o, a = stack.pop(), code.names[arg]
            stack.append(Sym(f"{o.name}.{a}") if isinstance(o, Sym) else getattr(o, a))
        elif n == "STORE_ATTR":
            o = stack.pop()
            setattr(o, code.names[arg], stack.pop())
        elif n in ("BUILD_TUPLE", "BUILD_LIST"):
            items = stack[len(stack) - arg:]
            del stack[len(stack) - arg:]
            stack.append(tuple(items) if n == "BUILD_TUPLE" else items)
        elif n == "CALL_FUNCTION":
            kw = {}
            for _ in range(arg >> 8):
                v = stack.pop()
                kw[stack.pop()] = v
            npos = arg & 0xFF
            pos = stack[len(stack) - npos:]
            del stack[len(stack) - npos:]
            f = stack.pop()
            v = Sym(f"{f.name}()") if isinstance(f, Sym) else f(*pos, **kw)
            if isinstance(v, Mapping) and not isinstance(v.callback, Method):
                sys.exit(f"a mapping's callback is {v.callback!r}; model how the client names it")
            stack.append(v)
        elif n == "POP_TOP":
            stack.pop()
        elif n in ("POP_JUMP_IF_FALSE", "POP_JUMP_IF_TRUE"):
            v = stack.pop()
            if isinstance(v, Sym):
                truth = not any(f in v.name for f in FALSE)
                taken.append((v.name, truth))
            else:
                truth = bool(v)
            if truth == (n == "POP_JUMP_IF_TRUE"):
                i = arg
        elif n == "JUMP_FORWARD":
            i += arg
        elif n == "JUMP_ABSOLUTE":
            i = arg
        elif n in ("SETUP_LOOP", "POP_BLOCK"):
            pass
        elif n == "GET_ITER":
            stack.append(iter(stack.pop()))
        elif n == "FOR_ITER":
            try:
                stack.append(next(stack[-1]))
            except StopIteration:
                stack.pop()
                i += arg
        elif n in ("BINARY_MODULO", "BINARY_AND", "INPLACE_ADD"):
            r, l = stack.pop(), stack.pop()
            if n == "BINARY_AND":  # `session.role & ROLE_X`: not a developer
                stack.append(0 if isinstance(l, Sym) or isinstance(r, Sym) else l & r)
            else:
                stack.append(l % r if n == "BINARY_MODULO" else l + r)
        elif n == "RETURN_VALUE":
            return stack.pop()
        else:
            sys.exit(f"{code.name}: opcode {op} at {i} is not modelled; the client changed")


# --- static reads: labels, categories, imports ------------------------------

STATIC_NAME = {54: "STORE_MAP", 90: "STORE_NAME", 95: "STORE_ATTR", 100: "LOAD_CONST",
               101: "LOAD_NAME", 105: "BUILD_MAP", 106: "LOAD_ATTR", 108: "IMPORT_NAME",
               109: "IMPORT_FROM"}


def instructions(code):
    """(opname, argument) per instruction, with names and consts resolved."""
    b, i, out = code.code.encode("latin-1"), 0, []
    while i < len(b):
        op, arg = b[i], None
        if op >= 90:
            arg, i = b[i + 1] | b[i + 2] << 8, i + 3
            if op == 100:
                arg = code.consts[arg]
            elif op in (90, 95, 101, 106, 108, 109):
                arg = code.names[arg]
        else:
            i += 1
        out.append((STATIC_NAME.get(op, op), arg))
    return out


def literal_dict(code, name):
    """`name = {const: const, ...}` at the top level of `code`."""
    d, pending = None, []
    for op, arg in instructions(code):
        if op == "BUILD_MAP":
            d, pending = {}, []
        elif op == "LOAD_CONST" and d is not None:
            pending.append(arg)
        elif op == "STORE_MAP" and d is not None and len(pending) >= 2:
            key, value = pending.pop(), pending.pop()
            d[key] = value
        elif op == "STORE_NAME" and arg == name and d is not None:
            return d
        else:
            d, pending = None, []
    return {}


def imports(code):
    """`from mod import Name [as Alias]` at module level: alias -> (mod, Name)."""
    out, mod, name = {}, None, None
    for op, arg in instructions(code):
        if op == "IMPORT_NAME":
            mod = arg
        elif op == "IMPORT_FROM":
            name = arg
        elif op == "STORE_NAME" and name:
            out[arg], name = (mod, name), None
    return out


def caption_path(ccp, mod, cls):
    """A window class's own `default_captionLabelPath`, or None."""
    try:
        body = child(load(ccp, mod.replace(".", "/") + ".pyj"), cls)
    except (KeyError, StopIteration):
        return None
    ins = instructions(body)
    for (op, arg), (op2, arg2) in zip(ins, ins[1:]):
        if op == "LOAD_CONST" and op2 == "STORE_NAME" and arg2 == "default_captionLabelPath":
            return arg
    return None


def name_label_paths(ccp, body, module):
    """`X.nameLabelPath = '<path>'` or `= Window.default_captionLabelPath` in a class body."""
    out, ins, imported = {}, instructions(body), imports(module)
    for k in range(3, len(ins)):
        if ins[k] != ("STORE_ATTR", "nameLabelPath") or ins[k - 1][0] != "LOAD_NAME":
            continue
        target = ins[k - 1][1]
        if ins[k - 2][0] == "LOAD_CONST":
            out[target] = ins[k - 2][1]
        elif ins[k - 2] == ("LOAD_ATTR", "default_captionLabelPath") and ins[k - 3][0] == "LOAD_NAME":
            src = imported.get(ins[k - 3][1])
            if src:
                out[target] = caption_path(ccp, *src)
    return out


def localization(ccp_path):
    """Label path ('UI/Commands/OpenMonitor') -> en-US text, from the SharedCache."""
    tq = os.path.dirname(ccp_path)
    want = {"res:/localizationfsd/localization_fsd_main.pickle": "main",
            "res:/localizationfsd/localization_fsd_en-us.pickle": "en"}
    files = {}
    with open(os.path.join(tq, "resfileindex.txt"), encoding="utf-8", errors="replace") as fh:
        for line in fh:
            res, rel = line.split(",", 2)[:2]
            if res in want:
                files[want[res]] = os.path.join(os.path.dirname(tq), "ResFiles", rel)
    with open(files["main"], "rb") as fh:
        labels = pickle.load(fh, encoding="latin-1")["labels"]
    with open(files["en"], "rb") as fh:
        en = pickle.load(fh, encoding="latin-1")[1]
    return {f"{v['FullPath']}/{v['label']}": en[mid][0] for mid, v in labels.items() if mid in en}


# --- driver ------------------------------------------------------------------

def find_ccp():
    roots = [f"{d}:\\" for d in string.ascii_uppercase if os.path.isdir(f"{d}:\\")] or ["/"]
    for root in roots:
        for cand in ("SharedCache", os.path.join("Games", "EVE", "SharedCache"),
                     os.path.join("EVE Shared Cache", "SharedCache")):
            p = os.path.join(root, cand, "tq", "code.ccp")
            if os.path.isfile(p):
                return p
    return None


def canonical(shortcut):
    keys = list(shortcut) if isinstance(shortcut, tuple) else [shortcut]
    mods = sorted((k for k in keys if k in MOD_ORDER), key=MOD_ORDER.index)
    return mods + [k for k in keys if k not in MOD_ORDER]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ccp", help="path to SharedCache/tq/code.ccp")
    args = ap.parse_args()
    path = args.ccp or find_ccp()
    if not path:
        sys.exit("could not find code.ccp; pass --ccp")
    ccp = zipfile.ZipFile(path)

    core_mod = load(ccp, "carbonui/services/command.pyj")
    eve_mod = load(ccp, "eve/client/script/ui/eveCommands.pyj")
    vk = {k: v for k, v in module_ints(load(ccp, "carbonui/uiconst.pyj")).items() if k.startswith("VK_")}
    uiconst = Obj(vk, lambda n: sys.exit(f"uiconst.{n} unknown"))
    globs = {
        "uiconst": uiconst, "xrange": range, "getattr": getattr, "bool": bool,
        "True": True, "False": False, "None": None,
        "CommandMapping": Mapping, "command": Obj({"CommandMapping": Mapping}),
        "session": Obj({"role": Sym("session.role")}),
    }
    self_ = Obj({}, Method)
    taken = []
    maps = run(child(child(core_mod, "CommandService"), "SetDefaultShortcutMappingCORE"), [self_], globs, taken)
    maps += run(child(child(eve_mod, "EveCommandService"), "SetDefaultShortcutMappingGAME"), [self_], globs, taken)

    for cond, truth in taken:
        print(f"  branch {cond} -> {truth}", file=sys.stderr)
    rebindable = [m for m in maps if not m.isLocked]
    # null = a command EVE ships unbound, distinct from one this table lacks.
    defaults = {m.callback.name: canonical(m.shortcut) if m.shortcut is not None else None
                for m in rebindable}

    # FuncToDesc's order: labelsByFuncName, then the callback's nameLabelPath.
    text = localization(path)
    by_func = {**literal_dict(core_mod, "labelsByFuncName"), **literal_dict(eve_mod, "labelsByFuncName")}
    by_attr = {**name_label_paths(ccp, child(core_mod, "CommandService"), core_mod),
               **name_label_paths(ccp, child(eve_mod, "EveCommandService"), eve_mod)}
    tabs = {cat: text.get(p, cat) for cat, p in literal_dict(eve_mod, "CATEGORIES").items()}
    names, unlabelled = {}, []
    for m in rebindable:
        cmd = m.callback.name
        names[cmd] = {"group": tabs[m.category]}
        label = text.get(by_func.get(cmd) or by_attr.get(cmd) or "")
        if label:
            names[cmd]["label"] = label
        else:
            unlabelled.append(cmd)

    for out, data in ((OUT_DEFAULTS, defaults), (OUT_NAMES, names)):
        with open(out, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(data, fh, indent=2, sort_keys=True, ensure_ascii=False)
            fh.write("\n")
    bound = sum(v is not None for v in defaults.values())
    print(f"{len(defaults)} rebindable commands, {bound} bound by default -> {OUT_DEFAULTS}", file=sys.stderr)
    print(f"{len(names) - len(unlabelled)} labelled -> {OUT_NAMES}", file=sys.stderr)
    print(f"no label (the app de-camelcases): {', '.join(unlabelled)}", file=sys.stderr)
    print(f"tabs: {', '.join(tabs.values())}", file=sys.stderr)


if __name__ == "__main__":
    main()
