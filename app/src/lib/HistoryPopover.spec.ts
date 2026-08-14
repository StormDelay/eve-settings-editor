// History replaces the permanent 280px backups column, and makes its subject
// unambiguous by stopping having one: it asks for every OPEN slot and renders
// one titled group each. `page.spec.ts` pins that the content is identical on
// every tab; these are the grouping and routing cases.
import { expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import HistoryPopover from "$lib/HistoryPopover.svelte";
import { calls } from "$lib/test/setup";
import { accountsStore } from "$lib/accounts.svelte";
import { names } from "$lib/names.svelte";
import { subject } from "$lib/subject.svelte";
import { answer, pending } from "$lib/ui/confirm.svelte";
import type { OpenOutcome, Slot } from "$lib/api";

// No dialog mock: restore's confirmation is in-app now, and answering it is
// answering `confirm.svelte`'s queue. The mock is gone rather than kept
// harmlessly, because its absence is what asserts nothing native is left here.
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

const opened = (file_name: string): OpenOutcome => ({
  status: "opened",
  path: `/eve/${file_name}`,
  file_name,
  fidelity: { state: "editable" },
  tree: {
    label: "root", kind: "dict", display: "{}", path: [],
    editable: false, edit_text: null, removable: false, in_shared: false, children: [],
  },
});

const backup = (file_name: string) => ({ path: `/eve/backups/${file_name}`, file_name, size: 2048 });

function mount(onRestored: (slot: Slot, o: OpenOutcome) => void = () => {}) {
  // `props:` explicitly — `anchor` is also a Svelte MOUNT option, so a bare
  // props object with that key is read as component options instead.
  render(HistoryPopover, { props: { anchor: document.body, onclose: () => {}, onRestored } });
}

const groups = async () => {
  const box = await screen.findByRole("dialog", { name: "History" });
  return within(box).getAllByRole("heading");
};

test("two open slots render two groups, each headed by subject AND file", async () => {
  names[950] = { name: "Baguette Commander", category: "character" };
  accountsStore.roster = {
    accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950] }],
    unassigned: [],
  };
  subject.slots.char = opened("core_char_950.dat");
  subject.slots.user = opened("core_user_140.dat");
  calls.stub("list_file_backups", [backup("core_char_950.dat.20260811-143200.bak")]);
  mount();

  const hs = await groups();
  expect(hs).toHaveLength(2);
  // Both halves: the name says whose settings, the file name says what gets
  // overwritten. The old panel had a 0.85em, 0.7-opacity subtitle for one.
  expect(hs[0].textContent).toMatch(/Baguette Commander — core_char_950\.dat/);
  expect(hs[1].textContent).toMatch(/stormdelay2 — core_user_140\.dat/);
});

test("one open slot renders one group, not an empty second", async () => {
  subject.slots.char = opened("core_char_950.dat");
  calls.stub("list_file_backups", []);
  mount();
  expect(await groups()).toHaveLength(1);
});

test("a slot with no backups keeps its heading and says so", async () => {
  subject.slots.char = opened("core_char_950.dat");
  calls.stub("list_file_backups", []);
  mount();
  await groups();
  expect(screen.getByText(/no history yet/i)).toBeTruthy();
  expect(screen.getByText(/every save leaves a restorable copy/i)).toBeTruthy();
});

/**
 * Restore used to write back into `slots[active]` — the slot derived from the
 * current VIEW. Each group now carries its own, which is the value it should
 * always have had.
 */
test("each group's Restore targets that group's slot", async () => {
  subject.slots.char = opened("core_char_950.dat");
  subject.slots.user = opened("core_user_140.dat");
  calls.stub("list_file_backups", (args: Record<string, unknown> | undefined) =>
    args?.slot === "user" ? [backup("core_user_140.dat.20260811-143200.bak")] : [],
  );
  calls.stub("restore_backup", opened("core_user_140.dat"));
  let landed: Slot | null = null;
  mount((slot) => (landed = slot));

  const box = await screen.findByRole("dialog", { name: "History" });
  const restore = await within(box).findByRole("button", { name: "Restore" });
  await fireEvent.click(restore);

  // The confirmation is in-app now, so answering it is answering the queue
  // rather than a mocked native call. Asserting the queue filled at all is the
  // half that matters: a restore that stopped asking would still route right.
  await waitFor(() => expect(pending).toHaveLength(1));
  expect(pending[0].title).toMatch(/^Restore /);
  answer(pending[0].id, true);

  await waitFor(() => expect(landed).toBe("user"));
  expect(calls.only("restore_backup").args).toMatchObject({ slot: "user" });
});

/**
 * Confirming inside the dialog must not dismiss the popover underneath it.
 *
 * A native `ask()` raised an OS window and produced no DOM pointer events at
 * all. An in-app confirmation raises a Sheet in a SEPARATE subtree, so its own
 * buttons looked exactly like a click on the page behind — and Popover closes on
 * any window pointerdown it did not stop itself. Clicking "Restore" in the
 * confirmation made History vanish under the dialog, which reads as "the restore
 * did nothing".
 *
 * The fix is a containment test in Popover, so this asserts the mechanism rather
 * than the symptom: a pointerdown inside a `[data-modal]` that does not contain
 * the popover is not an outside click.
 */
test("a click inside a modal above it does not close the popover", async () => {
  subject.slots.char = opened("core_char_950.dat");
  calls.stub("list_file_backups", []);
  let closed = false;
  render(HistoryPopover, {
    props: { anchor: document.body, onclose: () => (closed = true), onRestored: () => {} },
  });
  await screen.findByRole("dialog", { name: "History" });

  // A modal layer over the top, as ConfirmDialog's Sheet renders it.
  const modal = document.createElement("div");
  modal.setAttribute("data-modal", "");
  const button = document.createElement("button");
  modal.appendChild(button);
  document.body.appendChild(modal);

  await fireEvent.pointerDown(button);
  expect(closed, "the confirmation's own click is not an outside click").toBe(false);

  // A click that is genuinely outside still dismisses.
  await fireEvent.pointerDown(document.body);
  expect(closed).toBe(true);
  modal.remove();
});
