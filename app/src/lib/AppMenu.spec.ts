// The other half of the migration test. `Sidebar.spec.ts` asserts the five
// global actions are GONE from the sidebar; this asserts they arrived here.
// The pair is what stops the migration silently dropping one on the floor.
import { expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import AppMenu from "$lib/AppMenu.svelte";
import { calls } from "$lib/test/setup";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(null),
  ask: () => Promise.resolve(true),
  message: () => Promise.resolve(),
  confirm: () => Promise.resolve(true),
}));

function mount(handlers: Partial<Record<"onShowAccounts" | "onShowBatch" | "onShowAbout", () => void>> = {}) {
  // `props:` explicitly — `anchor` is also a Svelte MOUNT option, so a bare
  // props object with that key is read as component options instead.
  render(AppMenu, {
    props: {
      anchor: document.body,
      onclose: () => {},
      onShowAccounts: handlers.onShowAccounts ?? (() => {}),
      onShowBatch: handlers.onShowBatch ?? (() => {}),
      onShowAbout: handlers.onShowAbout ?? (() => {}),
    },
  });
}

test("all five migrated actions are here", async () => {
  calls.stub("launcher_proposals", []);
  mount();
  for (const name of [/accounts/i, /copy settings/i, /refresh names/i, /rescan profiles/i, /about/i]) {
    expect(await screen.findByRole("menuitem", { name })).toBeTruthy();
  }
});

test("each navigation item reports its choice", async () => {
  calls.stub("launcher_proposals", []);
  let hit: string | null = null;
  mount({
    onShowAccounts: () => (hit = "accounts"),
    onShowBatch: () => (hit = "batch"),
    onShowAbout: () => (hit = "about"),
  });
  await fireEvent.click(await screen.findByRole("menuitem", { name: /copy settings/i }));
  expect(hit).toBe("batch");
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
  expect(item.textContent?.trim()).toBe("Accounts…");
});

/** The scan is paid for on demand — opening the menu — and never at app start.
 *  It reads and UTF-8-decodes every `.log` in the launcher's directory. */
test("the launcher log is read when the menu opens, once", async () => {
  calls.stub("launcher_proposals", []);
  mount();
  await waitFor(() => expect(calls.of("launcher_proposals").length).toBe(1));
});
