// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
//
// Contract test for the one boundary nothing else can see. `invoke` is
// stringly-typed: the command name and every argument name are plain strings
// that TypeScript never checks and Rust never sees. Rename a command or an
// argument on either side and everything compiles, `svelte-check` passes, and
// the feature fails at runtime with "command not found" — the class of bug that
// only shows up in a live smoke.
//
// This reads both sides of the boundary and pins them together:
//   1. every command api.ts calls exists as a #[tauri::command]
//   2. every #[tauri::command] is registered in generate_handler!
//   3. every command Rust defines is actually reachable from api.ts
//   4. argument names agree, allowing for Rust snake_case -> JS camelCase
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { check } from "./test/check.ts";
const at = (p: string) => fileURLToPath(new URL(p, import.meta.url));

const apiSrc = readFileSync(at("./api.ts"), "utf8");
// Comments are stripped first: ops.rs's module doc mentions
// `#[tauri::command]` in prose, and a scanner that trusted it would parse the
// next unrelated `fn` as a command.
const stripComments = (s: string) =>
  s.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");

// Every module, not a hand-kept list: `#[tauri::command]` lives in lib.rs today
// and a named list was already one file behind when the batch half moved out of
// ops.rs into setup.rs. A scan that goes stale silently stops checking the
// commands it no longer reads, which is the one failure this test cannot afford.
const rustDir = at("../../src-tauri/src/");
const rustSrc = readdirSync(rustDir)
  .filter((f) => f.endsWith(".rs"))
  .map((f) => stripComments(readFileSync(rustDir + f, "utf8")))
  .join("\n");

const camel = (s: string) => s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());

// ---- the TypeScript side -------------------------------------------------

/// `invoke<Ret>("command", { a, b })` -> command name and argument names.
/// The generic never contains `(`, so stopping the scan there is safe.
function tsCommands(): Map<string, Set<string>> {
  const out = new Map<string, Set<string>>();
  const re = /invoke(?:<[^(]*?>)?\(\s*"([a-z0-9_]+)"\s*(?:,\s*\{([^}]*)\})?/g;
  for (const m of apiSrc.matchAll(re)) {
    const args = new Set(
      (m[2] ?? "")
        .split(",")
        .map((a) => a.split(":")[0].trim())
        .filter(Boolean),
    );
    out.set(m[1], args);
  }
  return out;
}

// ---- the Rust side -------------------------------------------------------

/// Tauri injects these; they are not part of the JS-visible argument list.
const INJECTED = /\bState\s*<|\bAppHandle\b|\bWindow\b|\bWebview/;

/// Split a Rust parameter list on top-level commas only — types contain commas
/// inside generics (`Result<A, B>`), which a naive split would break on.
function splitParams(list: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of list) {
    if (ch === "<" || ch === "(" || ch === "[") depth++;
    else if (ch === ">" || ch === ")" || ch === "]") depth--;
    if (ch === "," && depth === 0) {
      out.push(cur);
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur);
  return out;
}

function rustCommands(): Map<string, Set<string>> {
  const out = new Map<string, Set<string>>();
  const marker = "#[tauri::command]";
  let i = rustSrc.indexOf(marker);
  while (i !== -1) {
    const fnAt = rustSrc.indexOf("fn ", i);
    const open = rustSrc.indexOf("(", fnAt);
    const name = rustSrc.slice(fnAt + 3, open).trim();
    // Balance to the closing paren: parameter types nest.
    let depth = 0;
    let j = open;
    for (; j < rustSrc.length; j++) {
      if (rustSrc[j] === "(") depth++;
      else if (rustSrc[j] === ")" && --depth === 0) break;
    }
    const params = splitParams(rustSrc.slice(open + 1, j))
      .map((p) => p.trim())
      .filter((p) => p && !INJECTED.test(p))
      .map((p) => p.split(":")[0].trim())
      .filter(Boolean);
    out.set(name, new Set(params.map(camel)));
    i = rustSrc.indexOf(marker, j);
  }
  return out;
}

/// Names listed in `tauri::generate_handler![...]`.
function registered(): Set<string> {
  const m = rustSrc.match(/generate_handler!\s*\[([\s\S]*?)\]/);
  if (!m) throw new Error("no generate_handler! block found");
  return new Set(m[1].split(",").map((s) => s.trim()).filter(Boolean));
}

// ---- the contract --------------------------------------------------------

const ts = tsCommands();
const rs = rustCommands();
const reg = registered();

check("api.ts declares commands", ts.size > 0);
check("the Rust side declares commands", rs.size > 0);

const missingInRust = [...ts.keys()].filter((c) => !rs.has(c));
check(
  `every command api.ts calls exists in Rust${missingInRust.length ? ` (missing: ${missingInRust.join(", ")})` : ""}`,
  missingInRust.length === 0,
);

const unregistered = [...rs.keys()].filter((c) => !reg.has(c));
check(
  `every #[tauri::command] is in generate_handler!${unregistered.length ? ` (missing: ${unregistered.join(", ")})` : ""}`,
  unregistered.length === 0,
);

const unreachable = [...rs.keys()].filter((c) => !ts.has(c));
check(
  `every Rust command is reachable from api.ts${unreachable.length ? ` (dead: ${unreachable.join(", ")})` : ""}`,
  unreachable.length === 0,
);

const argMismatches: string[] = [];
for (const [cmd, tsArgs] of ts) {
  const rsArgs = rs.get(cmd);
  if (!rsArgs) continue; // already reported above
  const missing = [...tsArgs].filter((a) => !rsArgs.has(a));
  const extra = [...rsArgs].filter((a) => !tsArgs.has(a));
  if (missing.length || extra.length) {
    argMismatches.push(
      `${cmd}: api.ts sends [${[...tsArgs].join(", ")}], Rust expects [${[...rsArgs].join(", ")}]`,
    );
  }
}
check(
  `argument names agree for every command${argMismatches.length ? `\n    ${argMismatches.join("\n    ")}` : ""}`,
  argMismatches.length === 0,
);

console.log(`  (${ts.size} commands pinned)`);
