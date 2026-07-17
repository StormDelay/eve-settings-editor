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

export interface StackField {
  text: string;
  path: NodePath;
}

export interface WindowRect {
  id: string;
  label: string;
  open: boolean;
  renderable: boolean;
  resolution_matches: boolean;
  geom: Geom | null;
  flags: BoolFlag[];
  stacks: StackField | null;
}

export interface WindowLayout {
  reference_w: number;
  reference_h: number;
  windows: WindowRect[];
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
  inherits: boolean;
  columns: OverviewColumn[];
}
export interface OverviewWindow {
  index: number;
  tab_indices: number[];
}
export interface OverviewColumns {
  tabs: OverviewTab[];
  windows: OverviewWindow[];
}

export interface RememberedList {
  widget: string;
  entries: string[];
}

export type Category = "layout" | "autofill";
export interface BatchCandidate {
  path: string;
  file_name: string;
  id: number | null;
  folder: string;
  same_folder: boolean;
}
export type BatchOp = { kind: "full_copy" } | { kind: "categories"; categories: Category[] };
export interface BatchTargetResult {
  path: string;
  ok: boolean;
  backup_path: string | null;
  error: string | null;
}

export type Slot = "char" | "user";

export const api = {
  discover: () => invoke<Profile[]>("discover_profiles"),
  open: (slot: Slot, path: string) => invoke<OpenOutcome>("open_file", { slot, path }),
  close: (slot: Slot) => invoke<void>("close_file", { slot }),
  mutate: (slot: Slot, mutation: Mutation) =>
    invoke<TreeNodeData>("apply_mutation", { slot, mutation }),
  save: (slot: Slot, force: boolean) => invoke<SaveReport>("save_document", { slot, force }),
  listBackups: (slot: Slot) => invoke<BackupInfo[]>("list_file_backups", { slot }),
  restoreBackup: (slot: Slot, backupPath: string) =>
    invoke<OpenOutcome>("restore_backup", { slot, backupPath }),
  windowLayout: (slot: Slot) => invoke<WindowLayout>("window_layout", { slot }),
  resolveCharacterNames: (ids: number[]) =>
    invoke<NameMap>("resolve_character_names", { ids }),
  refreshCharacterNames: (ids: number[]) =>
    invoke<NameMap>("refresh_character_names", { ids }),
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
  autofillLists: () => invoke<RememberedList[]>("autofill_lists"),
  setAutofillList: (widget: string, entries: string[]) =>
    invoke<RememberedList[]>("set_autofill_list", { widget, entries }),
  clearAllAutofill: () => invoke<RememberedList[]>("clear_all_autofill"),
  batchTargets: (sourcePath: string, allowOtherFolders: boolean) =>
    invoke<BatchCandidate[]>("batch_targets", { sourcePath, allowOtherFolders }),
  batchApply: (sourcePath: string, op: BatchOp, targets: string[]) =>
    invoke<BatchTargetResult[]>("batch_apply", { sourcePath, op, targets }),
};

export function errMessage(e: unknown): string {
  const err = e as ErrDto;
  return err && err.code ? `[${err.code}] ${err.message}` : String(e);
}
