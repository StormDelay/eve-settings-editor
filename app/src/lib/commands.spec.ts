// The tests that make the discovery rules mechanical rather than aspirational.
//
// Every one of these is a rule someone will break six months from now without
// noticing, in a commit that looks like it only adds a feature. That is what
// they are for — none of them tests behaviour a user can see, and all of them
// fail the build when the palette starts becoming the only route to something.
import { describe, expect, test } from "vitest";
import { COMMANDS, byId, haystack } from "$lib/commands";
import { score, rank } from "$lib/fuzzy";
import { accel, MOD } from "$lib/keys";
import { resetSubject, subject } from "$lib/subject.svelte";

describe("registry invariants", () => {
  /** Discovery rule 1, as one assertion. It is the rule the other three rest
   *  on: it means the palette can be missed entirely at zero cost, which is the
   *  only honest basis for shipping one. */
  test("nothing is palette-only", () => {
    for (const c of COMMANDS) {
      expect(c.homes.length, `${c.id} has no home outside the palette`).toBeGreaterThanOrEqual(1);
    }
  });

  test("ids are unique and stable-shaped", () => {
    const ids = COMMANDS.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id).toMatch(/^[a-z]+\.[a-zA-Z]+$/);
  });

  /** Sentence case: one leading capital, and no second capitalised word unless
   *  it is a proper noun. "Remove Window" was the headline casing bug and this
   *  is what stops the next one. */
  test("labels are sentence case", () => {
    const PROPER = new Set(["EVE", "Settings", "Editor", "Overview", "Autofill", "Keybinds", "Probes", "Layout", "Raw", "Accounts"]);
    for (const c of COMMANDS) {
      expect(c.label[0], `${c.id}`).toBe(c.label[0].toUpperCase());
      for (const w of c.label.split(/[\s—]+/).slice(1)) {
        if (w === "" || PROPER.has(w)) continue;
        expect(w[0], `${c.id}: "${w}" is capitalised mid-label`).toBe(w[0].toLowerCase());
      }
    }
  });

  test("no label uses three dots where it means an ellipsis", () => {
    for (const c of COMMANDS) expect(c.label).not.toContain("...");
  });

  test("no accelerator is bound twice", () => {
    const bound = COMMANDS.filter((c) => c.accel).map((c) => c.accel!);
    expect(new Set(bound).size).toBe(bound.length);
  });

  /** A disabled command with no reason is a bug, because the menu renders it as
   *  a tooltip and the palette as a subtitle — both would show nothing. */
  test("a disabled command gives a non-empty reason", () => {
    resetSubject();
    for (const c of COMMANDS) {
      const why = c.enabled();
      if (why === true) continue;
      expect(typeof why, `${c.id}`).toBe("string");
      expect(why.length, `${c.id} is disabled with an empty reason`).toBeGreaterThan(0);
    }
  });

  /** R7, at the source. No label, keyword or reason may contain a literal
   *  modifier name: `accel()` decides what a shortcut is CALLED, per platform,
   *  and a hardcoded "Ctrl" is simply wrong on macOS. */
  test("no user-facing string in the registry hardcodes a modifier", () => {
    resetSubject();
    for (const c of COMMANDS) {
      const why = c.enabled();
      const strings = [c.label, c.keywords ?? "", why === true ? "" : why];
      for (const s of strings) {
        expect(s, `${c.id}`).not.toMatch(/\bCtrl\b|\bCmd\b|⌘/);
      }
    }
  });

  /** Every accelerator goes through `accel()`, so it renders differently on
   *  macOS by construction. Asserting the shape is what catches one written by
   *  hand. */
  test("every accelerator is in this platform's form", () => {
    for (const c of COMMANDS.filter((x) => x.accel)) {
      expect(c.accel).toBe(MOD === "⌘" ? `⌘${c.accel!.slice(1)}` : `Ctrl+${c.accel!.slice(5)}`);
    }
  });

  test("byId finds every command and nothing else", () => {
    for (const c of COMMANDS) expect(byId(c.id)).toBe(c);
    expect(byId("nope.missing")).toBeUndefined();
  });
});

describe("what each predicate actually answers", () => {
  test("Save says which of its two reasons applies", () => {
    resetSubject();
    expect(byId("file.save")!.enabled()).toBe("Open a character first");

    subject.slots.char = { status: "parse_failed", path: "/x", message: "", offset: 0, hex_preview: "" };
    expect(byId("file.save")!.enabled()).toBe("Nothing has changed");
    resetSubject();
  });

  test("Go to Layout borrows the tab strip's own reason, so the two cannot disagree", () => {
    resetSubject();
    expect(byId("go.layout")!.enabled()).toBe("Open a character to edit its window layout.");
  });

  test("Go to Raw is always reachable, because it is the escape hatch", () => {
    resetSubject();
    expect(byId("go.raw")!.enabled()).toBe(true);
  });
});

describe("ranking", () => {
  /** The proposal's own worked example, as a test: typing a group's name
   *  surfaces that group's commands, because the group is in the haystack. */
  test("typing `overv` ranks the Overview command first", () => {
    const ranked = rank(COMMANDS, (c) => score("overv", c.label, haystack(c)));
    expect(ranked[0].id).toBe("go.overview");
  });

  test("a command's own name beats a command that merely shares its group", () => {
    const save = score("save", "Save", "File");
    const history = score("save", "Show file history", "backups restore File");
    expect(save).toBeGreaterThan(history);
  });

  test("an unmatched character is not a match at all", () => {
    expect(score("zzz", "Save", "File")).toBe(-Infinity);
    expect(rank(COMMANDS, (c) => score("zzzz", c.label, haystack(c)))).toEqual([]);
  });

  test("diacritics fold, so a typed plain letter finds an accented name", () => {
    expect(score("renee", "Renée Dubois")).toBeGreaterThan(0);
  });

  test("an empty query keeps every command, in registry order", () => {
    const ranked = rank(COMMANDS, (c) => score("", c.label, haystack(c)));
    expect(ranked.map((c) => c.id)).toEqual(COMMANDS.map((c) => c.id));
  });
});

describe("the keyboard map and the printed shortcut are one thing", () => {
  test("every command with an accelerator prints what `accel` builds", () => {
    for (const c of COMMANDS.filter((x) => x.accel)) {
      const key = c.accel!.replace(/^(Ctrl\+|⌘)/, "");
      expect(c.accel).toBe(accel(key));
    }
  });
});
