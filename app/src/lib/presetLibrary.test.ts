// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { aspectLabel, summarise } from "./presetLibrary.svelte.ts";
import type { PresetInfo } from "./api.ts";

import { check } from "./test/check.ts";

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
check("probe formations label", aspectLabel("probe_formations") === "Probe formations");
check(
  "probe formations summarise alongside others",
  summarise(info("P", ["keybinds", "probe_formations"])) === "Keybinds · Probe formations",
);

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
