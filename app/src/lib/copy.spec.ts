// Copy invariants, enforced by scanning the source rather than by rendering it.
//
// These are deliberately source scans and not component tests: the bug they
// guard against is a NEW string written six months from now, in a file no
// existing test mounts. Nothing else can see that.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import { errMessage, errText } from "$lib/api";

const SRC = join(import.meta.dirname, "..");

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = join(dir, e.name);
    if (e.isDirectory()) return walk(p);
    return /\.(svelte|ts)$/.test(e.name) ? [p] : [];
  });
}

/** Every source file that can hold a user-facing string. Tests are excluded:
 *  they quote the strings, and quoting one is not shipping it. */
const FILES = walk(SRC).filter((p) => !/\.(spec|test)\.ts$/.test(p));

/**
 * Lines with every comment stripped, so prose ABOUT a rule does not trip it.
 *
 * Block comments go first and across the whole file, because both kinds here
 * span lines — a `<!-- … -->` explaining why a shortcut is NOT hardcoded would
 * otherwise be reported as one that is. Newlines are preserved through the
 * strip so the reported line numbers stay true.
 */
function codeLines(path: string): { line: number; text: string }[] {
  const blank = (m: string) => m.replace(/[^\n]/g, "");
  return readFileSync(path, "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/<!--[\s\S]*?-->/g, blank)
    .split(/\r?\n/)
    .map((raw, i) => ({ line: i + 1, text: raw.replace(/\/\/.*$/, "") }))
    .filter((l) => l.text.trim() !== "");
}

/** The string literals on a line, so a rule about PROSE is not applied to
 *  syntax. `...` is spread as often as it is an ellipsis. */
function literals(text: string): string[] {
  return (text.match(/"[^"\n]*"|'[^'\n]*'|`[^`\n]*`/g) ?? []).map((s) => s.slice(1, -1));
}

describe("R7 — a shortcut is never baked into a string", () => {
  /**
   * The app ships on Windows, Linux and macOS, and `Ctrl` is simply wrong on the
   * third. Every rendered shortcut goes through `accel()`, so a literal in a
   * source line is either a mistake or a comment — and comments are stripped
   * above.
   */
  test("no shipped line contains a literal Ctrl or ⌘ outside `accel`", () => {
    // Two files legitimately say it.
    //
    // `keys.ts` is where `Ctrl` is DEFINED, and `keybinds.ts` labels the
    // modifier bits EVE stores in its OWN keybinding table — that is a fact
    // about the file being edited, not an accelerator this app offers, and
    // renaming it per platform would misreport what the game wrote.
    const ALLOWED = ["keys.ts", "keybinds.ts"];
    const offenders: string[] = [];
    for (const f of FILES) {
      if (ALLOWED.some((a) => f.endsWith(a))) continue;
      for (const { line, text } of codeLines(f)) {
        if (/\bCtrl\b|⌘/.test(text)) offenders.push(`${f.replace(SRC, "")}:${line} ${text.trim()}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});

describe("R2 — the ellipsis is one character", () => {
  test("no shipped line uses three dots", () => {
    const offenders: string[] = [];
    for (const f of FILES) {
      for (const { line, text } of codeLines(f)) {
        // `...` is spread and rest syntax at least as often as it is an
        // ellipsis, so only the string literals are examined.
        if (literals(text).some((s) => s.includes("..."))) {
          offenders.push(`${f.replace(SRC, "")}:${line} ${text.trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});

describe("§2.10 — the machine code is relegated, not deleted", () => {
  const err = { code: "conflict", message: "The file changed on disk." };

  test("errText gives the backend's prose alone", () => {
    expect(errText(err)).toBe("The file changed on disk.");
  });

  test("errMessage keeps the code, for the diagnosing reader", () => {
    expect(errMessage(err)).toBe("[conflict] The file changed on disk.");
  });

  test("both degrade to the raw value when there is no code to strip", () => {
    expect(errText("boom")).toBe("boom");
    expect(errMessage("boom")).toBe("boom");
  });
});

describe("the dialog diet, as a count", () => {
  /**
   * The headline claim, pinned. Seventy-three blocking native dialogs became
   * seven in-app modal surfaces; what is left of the dialog plugin is file
   * pickers, which were never in scope and are not message dialogs.
   */
  test("no shipped file calls message, confirm or ask", () => {
    const offenders: string[] = [];
    for (const f of FILES) {
      for (const { line, text } of codeLines(f)) {
        if (/\b(await\s+)?(message|ask)\(/.test(text) || /\bawait confirm\(/.test(text)) {
          offenders.push(`${f.replace(SRC, "")}:${line} ${text.trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  test("the only thing imported from the dialog plugin is a file picker", () => {
    const bad: string[] = [];
    for (const f of FILES) {
      for (const { line, text } of codeLines(f)) {
        if (!text.includes("@tauri-apps/plugin-dialog")) continue;
        if (/\b(message|confirm|ask)\b/.test(text)) bad.push(`${f.replace(SRC, "")}:${line}`);
      }
    }
    expect(bad).toEqual([]);
  });

  /** §2.8's bug, pinned by its own string: the claim was false, and the fix is
   *  that nothing says it any more. */
  test("nothing tells the user a document edit can't be undone", () => {
    const offenders: string[] = [];
    for (const f of FILES) {
      for (const { line, text } of codeLines(f)) {
        if (/can't be undone|cannot be undone/i.test(text)) {
          offenders.push(`${f.replace(SRC, "")}:${line} ${text.trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});

describe("the undo boundary (05b §8)", () => {
  /**
   * `Ctrl+Z` means the DOCUMENT stack and never reinterprets itself. So a toast
   * for something the document stack cannot reverse — a settings-preset delete,
   * a backup restore, a batch copy, an account pairing — must not offer Undo:
   * clicking it would revert an unrelated document edit while leaving the
   * pairing written.
   *
   * The mistake would be a wrong import rather than a subtle one, which is
   * exactly what a source scan catches. Every legitimate `undoAction()` lives in
   * a file that edits an open document.
   */
  const DOCUMENT_EDITORS = [
    "OverviewView.svelte",
    "OverviewFiltersTab.svelte",
    "LayoutView.svelte",
    "NeocomButtons.svelte",
    "undo.svelte.ts",
  ];

  test("only files that edit an open document mint an Undo action", () => {
    const offenders: string[] = [];
    for (const f of FILES) {
      if (DOCUMENT_EDITORS.some((d) => f.endsWith(d))) continue;
      for (const { line, text } of codeLines(f)) {
        if (text.includes("undoAction(")) {
          offenders.push(`${f.replace(SRC, "")}:${line} ${text.trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  /** The roster is not a settings document and never reaches `doc.value`, so
   *  `api.undo()` cannot reverse a pairing. Its remedy is `unpair`, on the card. */
  test("the accounts view never calls undo", () => {
    const accounts = FILES.filter((f) => f.endsWith("AccountsView.svelte"));
    expect(accounts.length).toBe(1);
    for (const { text } of codeLines(accounts[0])) {
      expect(text).not.toMatch(/\bundoAction\b|api\.undo\b/);
    }
  });
});
