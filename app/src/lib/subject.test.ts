// "Discard and open" must discard: the caller swaps one slot, pairing can keep
// the other, and the open clears the undo history that could reach its edits.
import { expect, test } from "vitest";

import { confirmDiscardIfDirty, subject } from "./subject.svelte.ts";
import { answer, pending } from "./ui/confirm.svelte.ts";
import { calls } from "$lib/test/setup";

const opened = (path: string) => ({ status: "opened", path, file_name: path, fidelity: "exact", tree: null }) as never;

function dirtyPair() {
  subject.slots.char = opened("core_char_90000001.dat");
  subject.slots.user = opened("core_user_90000010.dat");
  subject.dirty.char = false;
  subject.dirty.user = true;
  calls.stub("open_file", ({ path }: { path: string }) => opened(path));
}

test("confirming re-reads both open files, so the kept slot loses its edits too", async () => {
  dirtyPair();
  const ok = confirmDiscardIfDirty();
  await Promise.resolve();
  answer(pending[0].id, true);
  expect(await ok).toBe(true);
  expect(calls.of("open_file").map((c) => c.args)).toEqual([
    { slot: "char", path: "core_char_90000001.dat" },
    { slot: "user", path: "core_user_90000010.dat" },
  ]);
  expect(subject.dirty.user).toBe(false);
});

test("keeping editing touches nothing", async () => {
  dirtyPair();
  const ok = confirmDiscardIfDirty();
  await Promise.resolve();
  answer(pending[0].id, false);
  expect(await ok).toBe(false);
  calls.never("open_file");
  expect(subject.dirty.user).toBe(true);
});
