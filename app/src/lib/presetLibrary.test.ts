// Run: npm test (node --test). No framework, no @types/node — a throw is a
// failing exit code, which is all a runner needs.
import { aspectLabel, summarise } from "./presetLibrary.ts";
import type { PresetInfo } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const info = (name: string, aspects: PresetInfo["aspects"], full = false): PresetInfo => ({
  name,
  dir: `/data/presets/${name}`,
  char_path: `/data/presets/${name}/char.dat`,
  user_path: `/data/presets/${name}/user.dat`,
  modified_unix: 0,
  aspects,
  full,
  error: null,
});

check("layout label", aspectLabel("layout") === "Layout");
check("overview label", aspectLabel("overview") === "Overview");
check("autofill label", aspectLabel("autofill") === "Autofill");
check("keybinds label", aspectLabel("keybinds") === "Keybinds");
check("everything label", aspectLabel("everything") === "Everything");

// A full preset says so once, rather than listing every aspect it implies.
check(
  "a full preset summarises as Everything",
  summarise(info("F", ["layout", "overview", "everything"], true)) === "Everything",
);
check(
  "a pruned preset lists what it holds",
  summarise(info("P", ["layout", "keybinds"])) === "Layout · Keybinds",
);
check("a broken preset summarises as unreadable", summarise({ ...info("B", []), error: "boom" }) === "unreadable");
check("an empty preset says so", summarise(info("E", [])) === "empty");
