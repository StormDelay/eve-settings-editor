// Typed mirror of the Rust command surface. The JSON shapes are contracts
// pinned by settings-model unit tests — change them there first.
import { invoke } from "@tauri-apps/api/core";

export interface Step {
  s: string;
  i?: number;
}
export type NodePath = Step[];

export interface TreeNodeData {
  label: string | null;
  kind: string;
  display: string;
  path: NodePath;
  editable: boolean;
  edit_text: string | null;
  removable: boolean;
  in_shared: boolean;
  children: TreeNodeData[];
}

export type Fidelity =
  | { state: "editable" }
  | { state: "read_only"; reason: string };

export type OpenOutcome =
  | {
      status: "opened";
      path: string;
      file_name: string;
      fidelity: Fidelity;
      tree: TreeNodeData;
    }
  | {
      status: "parse_failed";
      path: string;
      offset: number;
      message: string;
      hex_preview: string;
    };

export interface SettingsFile {
  path: string;
  file_name: string;
  kind: "char" | "user" | "other";
  id: number | null;
  size: number;
  modified_unix: number | null;
}

export interface Profile {
  install: string;
  server: string;
  profile: string;
  dir: string;
  files: SettingsFile[];
}

export interface SaveReport {
  backup_path: string;
  bytes_written: number;
}

export interface BackupInfo {
  path: string;
  file_name: string;
  size: number;
}

export interface ResolvedName {
  name: string;
  category: string;
}
export type NameMap = Record<string, ResolvedName>;

export interface ErrDto {
  code: string;
  message: string;
}

export type NewValue =
  | { kind: "none" }
  | { kind: "bool"; v: boolean }
  | { kind: "int"; v: string }
  | { kind: "float"; v: string }
  | { kind: "str"; v: string }
  | { kind: "str_ucs2"; v: string }
  | { kind: "bytes_hex"; v: string }
  | { kind: "empty_dict" }
  | { kind: "empty_list" }
  | { kind: "empty_tuple" };

export type Mutation =
  | { op: "set_scalar"; path: NodePath; text: string }
  | { op: "remove_entry"; path: NodePath }
  | { op: "insert_dict_entry"; parent: NodePath; key: NewValue; value: NewValue }
  // Also inserts into tuples — they are editable sequences (see mutate.rs).
  | { op: "insert_list_item"; parent: NodePath; index: number; value: NewValue };

export interface Geom {
  x: number;
  y: number;
  w: number;
  h: number;
  screen_w: number;
  screen_h: number;
  x_path: NodePath;
  y_path: NodePath;
  w_path: NodePath;
  h_path: NodePath;
  screen_w_path: NodePath;
  screen_h_path: NodePath;
}

export type SetTarget =
  | { how: "set"; path: NodePath }
  | { how: "insert"; parent: NodePath; key: NewValue }
  | { how: "unavailable" };

export interface BoolFlag {
  name: string;
  value: boolean;
  set: SetTarget;
}

export type HudScope = "char" | "account";
export type HudKind = "float" | "int" | "bool";

export interface HudEntry {
  name: string;
  kind: HudKind;
  /** null when the key is absent or holds an unexpected wire kind — use `default`. */
  value: string | null;
  default: string;
  scope: HudScope;
  /** Informational only: unlike a window BoolFlag's `Insert` (which
   * insert_dict_entry can act on directly), a HUD field's `Insert` means the
   * leaf needs the `(timestamp, value)` wrapper, and for a point field, would
   * insert the same key twice. Only `api.setHudValue` may act on it. */
  set: SetTarget;
}

export interface Hud {
  entries: HudEntry[];
}

export type StackRole = "container" | "member";
export interface StackRef {
  container_id: string;
  role: StackRole;
}
export interface Stack {
  container_id: string;
  container_label: string;
  anchor_id: string;
  members: string[];
}

export interface WindowRect {
  id: string;
  label: string;
  /** EVE's own name for this window when the file has one; null otherwise. */
  name: string | null;
  open: boolean;
  renderable: boolean;
  resolution_matches: boolean;
  geom: Geom | null;
  flags: BoolFlag[];
  stack: StackRef | null;
}

export interface WindowLayout {
  reference_w: number;
  reference_h: number;
  windows: WindowRect[];
  stacks: Stack[];
}

export interface NeocomButton {
  index: number;
  id: string;
  btn_type: number;
  icon_path: string;
  /** 0 for a plain button; a folder's child count otherwise. */
  children: number;
}
export interface NeocomBar {
  buttons: NeocomButton[];
  /** The client's own baseline. Not the addable set — see neocom.ts. */
  original: NeocomButton[];
}

/** Per-channel chat window splits, from the ACCOUNT document. Editable via
 * `setChatSplits`; account-scoped, so a write is shared by every character
 * on the account. */
export interface ChatPanel {
  window_id: string;
  /** null = the player has never resized this channel's member list. */
  userlist_width: number | null;
  input_height: number | null;
}

export interface LayoutPrefs {
  clutter: string[];
  visible: string[];
  /** Whether the layout canvas draws each rectangle's internals. */
  detail: boolean;
  /** How many locked targets the canvas draws the target list at. */
  targets: number;
  /** How many effect icons the canvas draws under the ship HUD. */
  effects: number;
}
export interface Preferences {
  layout: LayoutPrefs;
}

export interface AccountView {
  user_id: number;
  alias: string | null;
  characters: number[];
}
export interface AccountRoster {
  accounts: AccountView[];
  unassigned: number[];
}
export interface CaptureResult {
  changed_chars: number[];
  changed_users: number[];
  detected: [number, number] | null;
}

export interface OverviewColumn {
  name: string;
  label: string;
  visible: boolean;
  width: number | null;
}
export interface OverviewTab {
  index: number;
  name: string;
  preset: string;
  inherits: boolean;
  columns: OverviewColumn[];
}
export interface OverviewWindow {
  index: number;
  tab_indices: number[];
}
export interface Preset {
  name: string;
  groups: number[];
  filtered_states: number[];
  always_shown_states: number[];
}
export interface StateSurface {
  enabled: number[];
  order: number[];
}
export interface Appearance {
  background: StateSurface;
  flag: StateSurface;
  colors: [number, [number, number, number, number]][];
  bools: [string, boolean][];
  defaulted: boolean;
}
export interface OverviewColumns {
  tabs: OverviewTab[];
  windows: OverviewWindow[];
  presets: Preset[];
  appearance: Appearance;
}

export type PackSummary = { sections: [string, number][]; ignored: string[] };
export type PackReport = { applied: string[]; warnings: string[] };
export type PackImportResult = { columns: OverviewColumns; report: PackReport };

export interface GroupEntry {
  id: number;
  name: string;
  category_id: number;
  category_name: string;
}

export interface RememberedList {
  widget: string;
  entries: string[];
}

export type Formation = {
  id: number;
  /** Metre offsets from the formation centre. X and Z are horizontal, Y is up. */
  probes: [number, number, number][];
  name: string;
  /** Metres, one per probe, positionally matching `probes`. The client sets
   * scan range per probe, so these are edited per row. */
  ranges: number[];
};
export type Formations = { formations: Formation[]; selected: number | null };

/** A formation as it travels between files: no id, because an id is
 * account-local and an import allocates a fresh one. */
export type FormationSpec = {
  name: string;
  probes: [number, number, number][];
  ranges: number[];
};

export type KeybindEntry = {
  command: string;
  /** null = unbound. Otherwise [17?, 18?, 16?, key]. */
  keys: number[] | null;
  /** The stored value was not a recognised binding; shown read-only. */
  malformed: boolean;
};
export type Keybinds = { entries: KeybindEntry[]; available: boolean };
export type SetKeybindResult = { keybinds: Keybinds; stolen: string[] };

export interface BatchTargetResult {
  path: string;
  ok: boolean;
  backup_path: string | null;
  error: string | null;
}

export type Aspect = "layout" | "overview" | "autofill" | "keybinds" | "probe_formations" | "everything";
export interface CharWrite {
  char_id: number;
  path: string;
  full_copy: boolean;
  resolution_mismatch: boolean;
}
export interface AccountWrite {
  user_id: number;
  path: string;
  full_copy: boolean;
  collateral_char_ids: number[];
}
export interface ExcludedTarget {
  char_id: number;
  reason: string;
}
export interface SetupPlan {
  char_writes: CharWrite[];
  account_writes: AccountWrite[];
  excluded: ExcludedTarget[];
  source_error: string | null;
}

export interface PresetInfo {
  name: string;
  dir: string;
  char_path: string;
  user_path: string;
  modified_unix: number | null;
  aspects: Aspect[];
  full: boolean;
  /** Set when a document failed to decode; the row is shown but not openable. */
  error: string | null;
}

/** The refreshed library plus the name the import actually landed under, which
 * may be suffixed if the file's own name was already taken. */
export interface PresetImport {
  name: string;
  presets: PresetInfo[];
}

export type BatchSource =
  | { kind: "character"; path: string }
  | { kind: "preset"; dir: string; anchor_dir: string };

export type Slot = "char" | "user";

export const api = {
  discover: () => invoke<Profile[]>("discover_profiles"),
  open: (slot: Slot, path: string) => invoke<OpenOutcome>("open_file", { slot, path }),
  close: (slot: Slot) => invoke<void>("close_file", { slot }),
  mutate: (slot: Slot, mutation: Mutation) =>
    invoke<TreeNodeData>("apply_mutation", { slot, mutation }),
  mutateMany: (slot: Slot, mutations: Mutation[]) =>
    invoke<TreeNodeData>("apply_mutations", { slot, mutations }),
  save: (slot: Slot, force: boolean) => invoke<SaveReport>("save_document", { slot, force }),
  listBackups: (slot: Slot) => invoke<BackupInfo[]>("list_file_backups", { slot }),
  restoreBackup: (slot: Slot, backupPath: string) =>
    invoke<OpenOutcome>("restore_backup", { slot, backupPath }),
  windowLayout: (slot: Slot) => invoke<WindowLayout>("window_layout", { slot }),
  preferences: () => invoke<Preferences>("preferences"),
  setPreferences: (prefs: Preferences) => invoke<void>("set_preferences", { prefs }),
  hud: () => invoke<Hud>("hud_layout"),
  setHudValue: (name: string, text: string) =>
    invoke<Hud>("set_hud_value", { name, text }),
  resolveCharacterNames: (ids: number[]) =>
    invoke<NameMap>("resolve_character_names", { ids }),
  refreshCharacterNames: (ids: number[]) =>
    invoke<NameMap>("refresh_character_names", { ids }),
  syncGroupCatalog: (knownIds: number[], relevantCategories: number[]) =>
    invoke<GroupEntry[]>("sync_group_catalog", { knownIds, relevantCategories }),
  accountRoster: () => invoke<AccountRoster>("account_roster"),
  setAccountAlias: (userId: number, alias: string | null) =>
    invoke<AccountRoster>("set_account_alias", { userId, alias }),
  confirmPairing: (charId: number, userId: number) =>
    invoke<AccountRoster>("confirm_pairing", { charId, userId }),
  unpairCharacter: (charId: number) =>
    invoke<AccountRoster>("unpair_character", { charId }),
  beginCapture: () => invoke<void>("begin_capture"),
  resolveCapture: () => invoke<CaptureResult>("resolve_capture"),
  overviewColumns: () => invoke<OverviewColumns>("overview_columns"),
  setOverviewVisible: (tabIndex: number, column: string, visible: boolean) =>
    invoke<OverviewColumns>("set_overview_visible", { tabIndex, column, visible }),
  setOverviewOrder: (tabIndex: number, order: string[]) =>
    invoke<OverviewColumns>("set_overview_order", { tabIndex, order }),
  setOverviewWidth: (tabIndex: number, column: string, width: number) =>
    invoke<OverviewColumns>("set_overview_width", { tabIndex, column, width }),
  tabCreate: (windowIdx: number, name: string, fromTab: number | null) =>
    invoke<OverviewColumns>("tab_create", { windowIdx, name, fromTab }),
  tabRename: (tabIdx: number, name: string) =>
    invoke<OverviewColumns>("tab_rename", { tabIdx, name }),
  tabDelete: (tabIdx: number) =>
    invoke<OverviewColumns>("tab_delete", { tabIdx }),
  tabReorder: (windowIdx: number, order: number[]) =>
    invoke<OverviewColumns>("tab_reorder", { windowIdx, order }),
  tabMove: (tabIdx: number, fromWindow: number, toWindow: number, pos: number) =>
    invoke<OverviewColumns>("tab_move", { tabIdx, fromWindow, toWindow, pos }),
  overviewWindowAdd: (name: string, fromTab: number | null) =>
    invoke<OverviewColumns>("overview_window_add", { name, fromTab }),
  overviewWindowRemove: (windowIdx: number) =>
    invoke<OverviewColumns>("overview_window_remove", { windowIdx }),
  overviewCreateWindowMapping: () => invoke<OverviewColumns>("overview_create_window_mapping"),
  presetCreate: (from: string, newName: string) =>
    invoke<OverviewColumns>("preset_create", { from, newName }),
  presetRename: (oldName: string, newName: string) =>
    invoke<OverviewColumns>("preset_rename", { oldName, newName }),
  presetDelete: (name: string) =>
    invoke<OverviewColumns>("preset_delete", { name }),
  tabSetPreset: (tabIdx: number, preset: string) =>
    invoke<OverviewColumns>("tab_set_preset", { tabIdx, preset }),
  presetSetGroups: (name: string, groups: number[]) =>
    invoke<OverviewColumns>("preset_set_groups", { name, groups }),
  presetFork: (tabIdx: number, name: string, groups: number[], filteredStates: number[], alwaysShownStates: number[]) =>
    invoke<OverviewColumns>("preset_fork", { tabIdx, name, groups, filteredStates, alwaysShownStates }),
  overviewSetStates: (which: "background" | "backgroundOrder" | "flag" | "flagOrder", ids: number[]) =>
    invoke<OverviewColumns>("overview_set_states", { which, ids }),
  overviewSetStateColor: (id: number, rgba: [number, number, number, number] | null) =>
    invoke<OverviewColumns>("overview_set_state_color", { id, rgba }),
  overviewSetBool: (key: string, on: boolean) =>
    invoke<OverviewColumns>("overview_set_bool", { key, on }),
  presetSetStates: (name: string, filtered: number[], alwaysShown: number[]) =>
    invoke<OverviewColumns>("preset_set_states", { name, filtered, alwaysShown }),
  autofillLists: () => invoke<RememberedList[]>("autofill_lists"),
  setAutofillList: (widget: string, entries: string[]) =>
    invoke<RememberedList[]>("set_autofill_list", { widget, entries }),
  clearAllAutofill: () => invoke<RememberedList[]>("clear_all_autofill"),
  keybinds: () => invoke<Keybinds>("keybinds"),
  setKeybind: (command: string, keys: number[] | null) =>
    invoke<SetKeybindResult>("set_keybind", { command, keys }),
  setupPreview: (source: BatchSource, targetCharPaths: string[], aspects: Aspect[], allowOtherFolders: boolean) =>
    invoke<SetupPlan>("setup_preview", { source, targetCharPaths, aspects, allowOtherFolders }),
  setupApply: (source: BatchSource, targetCharPaths: string[], aspects: Aspect[], allowOtherFolders: boolean) =>
    invoke<BatchTargetResult[]>("setup_apply", { source, targetCharPaths, aspects, allowOtherFolders }),
  // The overview view already owns `presetCreate`/`presetRename`/`presetDelete`
  // for EVE's own overview filter presets — these are the settings-preset
  // library, hence the longer names.
  settingsPresetList: () => invoke<PresetInfo[]>("settings_preset_list"),
  settingsPresetCreate: (name: string, aspects: Aspect[], overwrite: boolean) =>
    invoke<PresetInfo[]>("settings_preset_create", { name, aspects, overwrite }),
  settingsPresetRename: (oldName: string, newName: string) =>
    invoke<PresetInfo[]>("settings_preset_rename", { oldName, newName }),
  settingsPresetDelete: (name: string) =>
    invoke<PresetInfo[]>("settings_preset_delete", { name }),
  settingsPresetExport: (name: string, path: string) =>
    invoke<void>("settings_preset_export", { name, path }),
  settingsPresetImport: (path: string) =>
    invoke<PresetImport>("settings_preset_import", { path }),
  stackUnstack: (member: string) => invoke<WindowLayout>("stack_unstack", { member }),
  stackAdd: (member: string, container: string) => invoke<WindowLayout>("stack_add", { member, container }),
  stackReorder: (container: string, members: string[]) => invoke<WindowLayout>("stack_reorder", { container, members }),
  stackCreate: (member1: string, member2: string) => invoke<WindowLayout>("stack_create", { member1, member2 }),
  stackDeleteOrphans: () => invoke<WindowLayout>("stack_delete_orphans"),
  neocomBar: () => invoke<NeocomBar>("neocom_bar"),
  chatPanels: () => invoke<ChatPanel[]>("chat_panels"),
  setChatSplits: (ids: string[], userlistWidth: number | null, inputHeight: number | null) =>
    invoke<ChatPanel[]>("set_chat_splits", { ids, userlistWidth, inputHeight }),
  neocomReorder: (order: number[]) => invoke<NeocomBar>("neocom_reorder", { order }),
  neocomRemove: (index: number) => invoke<NeocomBar>("neocom_remove", { index }),
  neocomAdd: (id: string, btnType: number, iconPath: string) =>
    invoke<NeocomBar>("neocom_add", { id, btnType, iconPath }),
  neocomReset: () => invoke<NeocomBar>("neocom_reset"),
  probeFormations: () => invoke<Formations>("probe_formations"),
  /** `id: null` creates at the next free id. `ranges` is one per probe. */
  setProbeFormation: (
    id: number | null,
    name: string,
    probes: [number, number, number][],
    ranges: number[],
  ) => invoke<Formations>("set_probe_formation", { id, name, probes, ranges }),
  removeProbeFormation: (id: number) =>
    invoke<Formations>("remove_probe_formation", { id }),
  /** The shared YAML for these formations. Pure text — the caller supplies the
   * data, so Copy and Export can send an uncommitted draft. */
  probeYaml: (formations: FormationSpec[]) => invoke<string>("probe_yaml", { formations }),
  probeParseYaml: (text: string) => invoke<FormationSpec[]>("probe_parse_yaml", { text }),
  probeExport: (path: string, formations: FormationSpec[]) =>
    invoke<void>("probe_export", { path, formations }),
  probeImport: (path: string) => invoke<FormationSpec[]>("probe_import", { path }),
  /** Add at fresh ids, suffixing any name the account already holds. Never
   * replaces or deletes anything. */
  addProbeFormations: (formations: FormationSpec[]) =>
    invoke<Formations>("add_probe_formations", { formations }),
  packPreview: (path: string) => invoke<PackSummary>("pack_preview", { path }),
  packImport: (path: string) => invoke<PackImportResult>("pack_import", { path }),
  packExport: (path: string) => invoke<PackReport>("pack_export", { path }),
};

export function errMessage(e: unknown): string {
  const err = e as ErrDto;
  return err && err.code ? `[${err.code}] ${err.message}` : String(e);
}
