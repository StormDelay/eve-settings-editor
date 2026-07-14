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
  recent_sibling_writes: string[];
}

export interface BackupInfo {
  path: string;
  file_name: string;
  size: number;
}

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

export const api = {
  discover: () => invoke<Profile[]>("discover_profiles"),
  open: (path: string) => invoke<OpenOutcome>("open_file", { path }),
  close: () => invoke<void>("close_file"),
  mutate: (mutation: Mutation) => invoke<TreeNodeData>("apply_mutation", { mutation }),
  save: (force: boolean) => invoke<SaveReport>("save_document", { force }),
  listBackups: () => invoke<BackupInfo[]>("list_file_backups"),
  restoreBackup: (backupPath: string) =>
    invoke<OpenOutcome>("restore_backup", { backupPath }),
};

export function errMessage(e: unknown): string {
  const err = e as ErrDto;
  return err && err.code ? `[${err.code}] ${err.message}` : String(e);
}
