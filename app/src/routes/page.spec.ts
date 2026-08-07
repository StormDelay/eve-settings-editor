// Component test (vitest + jsdom).
//
// The app shell. It owns three things nothing below it can see: which SLOT a
// chosen file is opened into, whether Save is reachable, and which view tabs
// exist for what is currently open. Each is a rule about the whole application
// state rather than about any one panel, so this is the only place it can be
// pinned.
//
// Named `page.spec.ts`, not `+page.spec.ts`: SvelteKit claims the `+page.`
// prefix for route files and would try to treat a `+page.spec.ts` as one.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import Page from "./+page.svelte";
import { calls } from "$lib/test/setup";
import type { OpenOutcome, Profile, TreeNodeData } from "$lib/api";

// The shell sets the OS window title from the open document. Not part of what
// is under test, and jsdom has no Tauri window to set it on.
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTitle: () => Promise.resolve() }),
}));

// The native file picker. `picked` is what the next "Open file…" returns — the
// only way to reach an ACCOUNT file, which the sidebar deliberately does not
// list as a row (it lists characters; see Sidebar.spec.ts).
let picked: string | null = null;
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(picked),
  ask: () => Promise.resolve(true),
  message: () => Promise.resolve(),
  confirm: () => Promise.resolve(true),
}));

const tree: TreeNodeData = {
  label: "root", kind: "dict", display: "{}", path: [],
  editable: false, edit_text: null, removable: false, in_shared: false, children: [],
};

const opened = (file: string, readOnly = false): OpenOutcome => ({
  status: "opened",
  path: `/eve/${file}`,
  file_name: file,
  fidelity: readOnly ? { state: "read_only", reason: "unsupported opcode" } : { state: "editable" },
  tree,
});

const file = (file_name: string, kind: "char" | "user" | "other", id: number | null) => ({
  path: `/eve/${file_name}`, file_name, kind, id, size: 1024, modified_unix: 0,
});

const profile = (...files: ReturnType<typeof file>[]): Profile => ({
  install: "tq", server: "Tranquility", profile: "default", dir: "/eve", files,
});

/**
 * Mount the shell and wait for every call it fires on mount to have LANDED.
 *
 * Waiting for all of them matters, not just the one a test cares about: the
 * preferences and roster stores are module-level rune state shared by every
 * test in this file, and a load still in flight when `afterEach` clears the
 * stubs resolves to `undefined` and poisons that state for the next test.
 */
async function mount(...profiles: Profile[]) {
  calls.stub("discover_profiles", profiles);
  calls.stub("account_roster", { accounts: [], unassigned: [] });
  calls.stub("preferences", { layout: { clutter: [], visible: [], detail: false, targets: 4, effects: 2 } });
  calls.stub("settings_preset_list", []);
  // The backups panel is always mounted. An unstubbed command resolves to
  // `undefined`, which is not a shape the real backend can return — stub it
  // rather than teaching the panel to defend against an impossible reply.
  calls.stub("list_file_backups", []);
  render(Page);
  await waitFor(() => {
    expect(calls.of("discover_profiles").length).toBeGreaterThan(0);
    expect(calls.of("preferences").length).toBeGreaterThan(0);
  });
  // One more turn of the microtask queue, so the awaits inside those loaders
  // have assigned before the test body runs.
  await Promise.resolve();
}

/** Click the sidebar row for a settings file. */
async function openFile(name: string) {
  const row = await screen.findByText(name);
  await fireEvent.click(row);
}

const save = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;

describe("routing a file to its slot", () => {
  // `core_user_<id>.dat` is the account file; everything else is the char slot,
  // which is the generic editing one. The sidebar only lists a profile that has
  // a character file, so both fixtures carry one.
  const both = profile(
    file("core_char_950.dat", "char", 950),
    file("core_user_140.dat", "user", 140),
  );

  test("a character file opens into the char slot", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount(both);
    await openFile("core_char_950.dat");
    await waitFor(() => {
      expect(calls.of("open_file")[0].args).toMatchObject({
        slot: "char", path: "/eve/core_char_950.dat",
      });
    });
  });

  test("an account file picked from the dialog opens into the user slot", async () => {
    calls.stub("open_file", opened("core_user_140.dat"));
    picked = "/eve/core_user_140.dat";
    await mount(both);
    await fireEvent.click(screen.getByRole("button", { name: /open file/i }));
    await waitFor(() => {
      expect(calls.of("open_file")[0].args).toMatchObject({
        slot: "user", path: "/eve/core_user_140.dat",
      });
    });
    picked = null;
  });

  // Anything EVE did not name goes to the char slot, which is the generic
  // editing one — not to `user` and not to an error.
  test("a non-standard file opens into the char slot", async () => {
    calls.stub("open_file", opened("prefs.ini"));
    picked = "/eve/prefs.ini";
    await mount(both);
    await fireEvent.click(screen.getByRole("button", { name: /open file/i }));
    await waitFor(() => {
      expect(calls.of("open_file")[0].args).toMatchObject({
        slot: "char", path: "/eve/prefs.ini",
      });
    });
    picked = null;
  });
});

describe("saving", () => {
  // With nothing open there is no toolbar at all, so there is no Save to reach
  // — the shell shows the hint instead.
  test("nothing open means no toolbar to save from", async () => {
    await mount(profile(file("core_char_950.dat", "char", 950)));
    expect(screen.getByText(/open a settings file to begin/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  });

  // Opening a file is not an edit. Save must stay unreachable until something
  // has actually changed, or every open offers to rewrite an untouched file.
  test("Save stays unreachable on a freshly opened, unedited file", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount(profile(file("core_char_950.dat", "char", 950)));
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));
    expect(save().disabled).toBe(true);
  });

  // A file the codec cannot round-trip is shown read-only (spec §7). It can
  // become dirty in no way, and Save must never light up for it.
  test("Save is unreachable for a read-only document", async () => {
    calls.stub("open_file", opened("core_char_950.dat", true));
    await mount(profile(file("core_char_950.dat", "char", 950)));
    await openFile("core_char_950.dat");
    await screen.findByText("read-only");
    expect(save().disabled).toBe(true);
  });
});

describe("the view tabs", () => {
  // A non-standard file opens into the char slot but yields no character id and
  // no windows, so every view but the tree stays hidden — including Tree's own
  // strip, which only appears once there is a second view to switch to.
  test("a file with no character id offers no view tabs", async () => {
    calls.stub("open_file", opened("prefs.ini"));
    await mount(profile(file("core_char_950.dat", "char", 950)));
    await openFile("core_char_950.dat");
    await screen.findByText("editable");
    for (const v of ["Tree", "Layout", "Overview", "Autofill", "Keybinds", "Probes"]) {
      expect(screen.queryByRole("button", { name: v })).toBeNull();
    }
  });

  // The account-scoped editors need an id to work from — a character id or an
  // open account file. A `core_char_<id>.dat` supplies one.
  test("a character file unlocks the account-scoped editors", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount(profile(file("core_char_950.dat", "char", 950)));
    await openFile("core_char_950.dat");
    for (const v of ["Overview", "Autofill", "Keybinds", "Probes"]) {
      expect(await screen.findByRole("button", { name: v })).toBeTruthy();
    }
  });

  // Layout is gated on the document actually having windows, not merely on a
  // file being open — an account file has none.
  test("Layout stays hidden for a document with no windows", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    calls.stub("window_layout", { reference_w: 0, reference_h: 0, windows: [], stacks: [] });
    await mount(profile(file("core_char_950.dat", "char", 950)));
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("window_layout").length).toBeGreaterThan(0));
    expect(screen.queryByRole("button", { name: "Layout" })).toBeNull();
  });
});
