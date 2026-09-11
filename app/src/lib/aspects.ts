// The six things a preset or a batch copy can carry, named once.
//
// There were two label sets for this one six-item concept — PresetGroup's bare
// nouns and BatchView's fuller phrases — sitting in two files that a user moves
// between while doing the same job. The fuller set wins: "Window layout" alone
// does not tell you the neocom comes with it, and that is exactly the surprise a
// copy has to not spring.
import type { Aspect } from "./api";

export const ASPECT_LABELS: { key: Aspect; label: string }[] = [
  { key: "layout", label: "Window layout (positions, neocom, ship HUD, fighter panel, badge)" },
  { key: "overview", label: "Overview (columns, tabs, presets)" },
  { key: "autofill", label: "Autofill (remembered text)" },
  { key: "keybinds", label: "Keybindings" },
  { key: "probe_formations", label: "Probe formations (custom scan formations)" },
  { key: "everything", label: "Everything (full clone of both files)" },
];
