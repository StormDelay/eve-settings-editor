// The other half of the migration test. `Sidebar.spec.ts` asserts the five
// global actions are GONE from the sidebar; this asserts they arrived here.
// The pair is what stops the migration silently dropping one on the floor.
import { expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import AppMenu from "$lib/AppMenu.svelte";
import type { Ctx } from "$lib/commands";
import { accel } from "$lib/keys";
import { resetSubject } from "$lib/subject.svelte";
import { calls } from "$lib/test/setup";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(null),
  ask: () => Promise.resolve(true),
  message: () => Promise.resolve(),
  confirm: () => Promise.resolve(true),
}));

// The menu is built from the command registry now, so its "handlers" are the
// registry's own action context. That is the point: the menu cannot list a
// command the registry does not have, or omit one it does.
const noop = () => {};
const baseCtx: Ctx = {
  goto: noop,
  pickFile: noop,
  save: noop,
  discard: noop,
  showHistory: noop,
  showAccounts: noop,
  showBatch: noop,
  showAbout: noop,
  showShortcuts: noop,
  openPalette: noop,
  findInView: noop,
};

function mount(over: Partial<Ctx> = {}) {
  // `props:` explicitly — `anchor` is also a Svelte MOUNT option, so a bare
  // props object with that key is read as component options instead.
  render(AppMenu, {
    props: { anchor: document.body, onclose: () => {}, ctx: { ...baseCtx, ...over } },
  });
}

test("all five migrated actions are here", async () => {
  calls.stub("launcher_proposals", []);
  mount();
  for (const name of [/accounts/i, /copy settings/i, /refresh character names/i, /rescan profiles/i, /about/i]) {
    expect(await screen.findByRole("menuitem", { name })).toBeTruthy();
  }
});

test("each navigation item reports its choice", async () => {
  calls.stub("launcher_proposals", []);
  let hit: string | null = null;
  mount({
    showAccounts: () => (hit = "accounts"),
    showBatch: () => (hit = "batch"),
    showAbout: () => (hit = "about"),
  });
  await fireEvent.click(await screen.findByRole("menuitem", { name: /copy settings/i }));
  expect(hit).toBe("batch");
});

/**
 * Discovery rule 3, made mechanical: every menu row that HAS an accelerator
 * prints it, per platform. People learn the shortcut at the moment they use the
 * slow path, and only if the slow path shows it.
 */
test("a row with an accelerator prints it", async () => {
  calls.stub("launcher_proposals", []);
  mount();
  const save = await screen.findByRole("menuitem", { name: /save/i });
  expect(save.textContent).toContain(accel("S"));
});

/**
 * Disabled with a reason, never hidden. A row that vanishes when the backend
 * would refuse it teaches nothing and moves the rows under the cursor.
 */
test("Save is present and disabled with its reason when nothing has changed", async () => {
  calls.stub("launcher_proposals", []);
  resetSubject();
  mount();
  const save = (await screen.findByRole("menuitem", { name: /save/i })) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  expect(save.title).toBe("Open a character first");
});

/**
 * §5.10.1. The one signpost that proposals are waiting, and the reason §5.7.1
 * can refuse a per-character chip in the sidebar without leaving the question
 * unanswered.
 *
 * It counts, it does not name — naming is `Accept all`'s job inside the sheet,
 * where the objects are on screen. And it attaches to no character's row, so
 * §5.7.2 rule 6 stands: no list gains an account chip from a `Proposal`.
 */
test("Accounts carries a count of the pairings the launcher proposes", async () => {
  calls.stub("launcher_proposals", [
    { char_id: 951, user_id: 140, conflict: null },
    { char_id: 952, user_id: 140, conflict: null },
    // Disputed, so it is not part of the count: a dispute is a different
    // question from "nobody has answered this yet".
    { char_id: 953, user_id: 141, conflict: 140 },
  ]);
  mount();
  const item = await screen.findByRole("menuitem", { name: /accounts/i });
  await waitFor(() => expect(item.textContent).toMatch(/2/));
});

test("no count is drawn when nothing is proposed", async () => {
  calls.stub("launcher_proposals", []);
  mount();
  const item = await screen.findByRole("menuitem", { name: /accounts/i });
  await waitFor(() => expect(calls.of("launcher_proposals").length).toBe(1));
  expect(item.textContent?.trim()).toBe("Accounts");
});

/** The scan is paid for on demand — opening the menu — and never at app start.
 *  It reads and UTF-8-decodes every `.log` in the launcher's directory. */
test("the launcher log is read when the menu opens, once", async () => {
  calls.stub("launcher_proposals", []);
  mount();
  await waitFor(() => expect(calls.of("launcher_proposals").length).toBe(1));
});
