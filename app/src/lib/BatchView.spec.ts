// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// Batch is the only destructive screen in the app — it overwrites other
// characters' settings files, several at once. The rules that decide WHICH
// files get written live entirely in this component and were covered by
// nothing: "Everything" being exclusive, unpaired characters being excluded
// from account-scoped aspects, and the apply call sending the *effective*
// targets rather than the raw checkbox set. Getting any of them wrong
// overwrites a file the user did not choose.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import BatchView from "$lib/BatchView.svelte";
import { calls } from "$lib/test/setup";
import type { AccountRoster, Profile, SetupPlan } from "$lib/api";

const DIR = "C:/eve/settings_Default";

function charFile(id: number) {
  return {
    path: `${DIR}/core_char_${id}.dat`,
    file_name: `core_char_${id}.dat`,
    kind: "char" as const,
    id,
    size: 1000,
    modified_unix: 0,
  };
}

const PROFILES: Profile[] = [
  {
    install: "eve",
    server: "tranquility",
    profile: "Default",
    dir: DIR,
    files: [charFile(90000001), charFile(90000002), charFile(90000003)],
  },
];

/// 90000001 and 90000002 are paired to an account; 90000003 is not.
const ROSTER: AccountRoster = {
  accounts: [{ user_id: 80000001, alias: "Main", characters: [90000001, 90000002] }],
  unassigned: [90000003],
};

const PLAN: SetupPlan = {
  char_writes: [{ path: `${DIR}/core_char_90000002.dat` } as never],
  account_writes: [],
  excluded: [],
  source_error: null,
};

async function mount(openPath: string | null = `${DIR}/core_char_90000001.dat`) {
  calls.stub("discover_profiles", PROFILES);
  calls.stub("account_roster", ROSTER);
  calls.stub("setup_apply", []);
  calls.stub("resolve_character_names", {});
  // Only the default: a test that stubbed its own plan before mounting keeps it.
  if (!calls.stubbed("setup_preview")) calls.stub("setup_preview", PLAN);
  render(BatchView, { openPath });
  // The component discovers profiles and the roster on mount; nothing renders
  // a target row until both land.
  await waitFor(() => expect(targetRow(90000002)).toBeTruthy());
}

/// The two panels have no distinguishing class, so they are found by the
/// heading above them. A filename appears in BOTH the source dropdown and the
/// target list, which is why nothing here queries the document globally.
function section(head: string): HTMLElement {
  const h = [...document.querySelectorAll(".head")].find((e) => e.textContent?.includes(head));
  if (!h) throw new Error(`no section headed "${head}"`);
  return h.closest("section") as HTMLElement;
}

function rowIn(head: string, text: string): HTMLLabelElement {
  const row = [...section(head).querySelectorAll("label")].find((l) =>
    l.textContent?.includes(text),
  );
  if (!row) throw new Error(`no row matching "${text}" under "${head}"`);
  return row as HTMLLabelElement;
}

const aspect = (label: string) =>
  rowIn("What to copy", label).querySelector("input")! as HTMLInputElement;

const targetRow = (id: number) => rowIn("Target characters", `core_char_${id}.dat`);

const targetBox = (id: number) => targetRow(id).querySelector("input")! as HTMLInputElement;

describe("aspect selection", () => {
  test("Everything clears the other aspects", async () => {
    await mount();
    await fireEvent.click(aspect("Window layout"));
    await fireEvent.click(aspect("Autofill (remembered text)"));
    expect(aspect("Window layout").checked).toBe(true);

    await fireEvent.click(aspect("Everything (full clone of both files)"));
    await waitFor(() => expect(aspect("Window layout").checked).toBe(false));
    expect(aspect("Autofill (remembered text)").checked).toBe(false);
    expect(aspect("Everything (full clone of both files)").checked).toBe(true);
  });

  test("picking another aspect clears Everything", async () => {
    await mount();
    await fireEvent.click(aspect("Everything (full clone of both files)"));
    await waitFor(() => expect(aspect("Window layout").disabled).toBe(true));

    await fireEvent.click(aspect("Everything (full clone of both files)"));
    await waitFor(() => expect(aspect("Window layout").disabled).toBe(false));
    await fireEvent.click(aspect("Window layout"));
    await waitFor(() => expect(aspect("Everything (full clone of both files)").checked).toBe(false));
  });
});

describe("which characters can be written", () => {
  test("an unpaired character is disabled once an account-scoped aspect is picked", async () => {
    await mount();
    expect(targetBox(90000003).disabled).toBe(false);

    await fireEvent.click(aspect("Overview (columns, tabs, presets)"));
    await waitFor(() => expect(targetBox(90000003).disabled).toBe(true));
    // A paired one stays available.
    expect(targetBox(90000002).disabled).toBe(false);
  });

  test("a layout-only selection warns that the account file is written", async () => {
    // Layout carries the account-side HUD fields now, so it must disable
    // unpaired targets the way every other account aspect does.
    await mount();
    expect(targetBox(90000003).disabled).toBe(false);

    await fireEvent.click(aspect("Window layout"));
    await waitFor(() => expect(targetBox(90000003).disabled).toBe(true));
    // A paired one stays available.
    expect(targetBox(90000002).disabled).toBe(false);
  });

  test("a disabled row hides its tick but keeps the selection for later", async () => {
    await mount();
    await fireEvent.click(targetBox(90000003));
    await waitFor(() => expect(targetBox(90000003).checked).toBe(true));

    // While excluded the box reads unchecked, so the UI never claims a file is
    // about to be written that the backend would refuse.
    await fireEvent.click(aspect("Overview (columns, tabs, presets)"));
    await waitFor(() => expect(targetBox(90000003).disabled).toBe(true));
    expect(targetBox(90000003).checked).toBe(false);

    // The selection itself survives: changing back re-includes it without the
    // user having to tick it again.
    await fireEvent.click(aspect("Overview (columns, tabs, presets)"));
    await waitFor(() => expect(targetBox(90000003).disabled).toBe(false));
    expect(targetBox(90000003).checked).toBe(true);
  });
});

describe("what actually gets written", () => {
  test("apply sends only the effective targets, never a disabled row", async () => {
    await mount();
    await fireEvent.click(targetBox(90000002));
    await fireEvent.click(targetBox(90000003));
    await fireEvent.click(aspect("Overview (columns, tabs, presets)"));
    await waitFor(() => expect(targetBox(90000003).disabled).toBe(true));

    const apply = screen.getByRole("button", { name: /apply/i });
    await waitFor(() => expect(apply.hasAttribute("disabled")).toBe(false));
    await fireEvent.click(apply);

    await waitFor(() => expect(calls.of("setup_apply").length).toBe(1));
    const sent = calls.only("setup_apply").args!;
    expect(sent.targetCharPaths).toEqual([`${DIR}/core_char_90000002.dat`]);
    expect(sent.source).toEqual({ kind: "character", path: `${DIR}/core_char_90000001.dat` });
    expect(sent.aspects).toEqual(["overview"]);
  });

  test("select-all skips rows the current aspect excludes", async () => {
    await mount();
    await fireEvent.click(aspect("Overview (columns, tabs, presets)"));
    await waitFor(() => expect(targetBox(90000003).disabled).toBe(true));

    await fireEvent.click(screen.getByRole("button", { name: /select all/i }));
    await waitFor(() => expect(targetBox(90000002).checked).toBe(true));
    expect(targetBox(90000003).checked).toBe(false);
  });

  test("apply is refused while the plan would write nothing", async () => {
    calls.stub("setup_preview", { ...PLAN, char_writes: [], account_writes: [] });
    await mount();
    await fireEvent.click(targetBox(90000002));
    await fireEvent.click(aspect("Window layout"));
    await waitFor(() => expect(calls.of("setup_preview").length).toBeGreaterThan(0));
    expect(screen.getByRole("button", { name: /apply/i }).hasAttribute("disabled")).toBe(true);
  });

  test("apply is refused when the source itself failed to read", async () => {
    calls.stub("setup_preview", { ...PLAN, source_error: "Decode failed" });
    await mount();
    await fireEvent.click(targetBox(90000002));
    await fireEvent.click(aspect("Window layout"));
    await waitFor(() => expect(calls.of("setup_preview").length).toBeGreaterThan(0));
    expect(screen.getByRole("button", { name: /apply/i }).hasAttribute("disabled")).toBe(true);
  });
});

test("a stale preview response cannot overwrite a newer one", async () => {
  // Two previews in flight; the FIRST resolves last. The component guards this
  // with a request token, and without it the plan on screen would describe an
  // aspect the user has already changed away from.
  const pending: ((p: SetupPlan) => void)[] = [];
  calls.stub("discover_profiles", PROFILES);
  calls.stub("account_roster", ROSTER);
  calls.stub("resolve_character_names", {});
  calls.stub("setup_apply", []);
  calls.stub(
    "setup_preview",
    () => new Promise<SetupPlan>((resolve) => pending.push(resolve)),
  );
  render(BatchView, { openPath: `${DIR}/core_char_90000001.dat` });
  await waitFor(() => expect(targetRow(90000002)).toBeTruthy());

  await fireEvent.click(targetBox(90000002));
  await fireEvent.click(aspect("Window layout"));
  await waitFor(() => expect(pending.length).toBe(1));

  await fireEvent.click(aspect("Autofill (remembered text)"));
  await waitFor(() => expect(pending.length).toBe(2));

  // Resolve the NEWER request first, then let the stale one land.
  pending[1]({ ...PLAN, source_error: null });
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /apply/i }).hasAttribute("disabled")).toBe(false),
  );
  pending[0]({ ...PLAN, source_error: "stale source error" });

  await new Promise((r) => setTimeout(r, 0));
  expect(screen.queryByText(/stale source error/)).toBeNull();
  expect(screen.getByRole("button", { name: /apply/i }).hasAttribute("disabled")).toBe(false);
});

test("changing the source clears the aspects and targets already picked", async () => {
  await mount();
  await fireEvent.click(targetBox(90000002));
  await fireEvent.click(aspect("Window layout"));
  await waitFor(() => expect(targetBox(90000002).checked).toBe(true));

  const sourceSelect = document.querySelector("#src") as HTMLSelectElement;
  await fireEvent.change(sourceSelect, { target: { value: `${DIR}/core_char_90000002.dat` } });

  await waitFor(() => expect(aspect("Window layout").checked).toBe(false));
  const stillTicked = [...section("Target characters").querySelectorAll("input")].filter(
    (i) => (i as HTMLInputElement).checked,
  );
  expect(stillTicked).toEqual([]);
});

test("the account warning says a layout copy can reset fields to EVE's defaults", async () => {
  // A Layout copy is the only one that REMOVES anything: a HUD field the source
  // leaves at EVE's default is deleted from the target so it falls back to the
  // same default. Nothing on screen said so, and "changed" does not cover it.
  calls.stub("setup_preview", {
    ...PLAN,
    account_writes: [
      { user_id: 80000001, path: `${DIR}/core_user_80000001.dat`, full_copy: false, collateral_char_ids: [] },
    ],
  } as never);
  await mount();
  await fireEvent.click(targetBox(90000002));
  await fireEvent.click(aspect("Window layout"));
  expect(await screen.findByText(/reset to that default/i)).toBeTruthy();

  // Autofill only ever overwrites, so the clause must not claim otherwise.
  await fireEvent.click(aspect("Window layout"));
  await fireEvent.click(aspect("Autofill (remembered text)"));
  await waitFor(() => expect(screen.queryByText(/reset to that default/i)).toBeNull());
  expect(screen.getByText(/Autofill \(remembered text\) changed/)).toBeTruthy();
});

test("warns when a target is the file currently open", async () => {
  // 90000001 is the open document. Point the source elsewhere so it becomes a
  // selectable target, then select it: the apply would write behind the copy on
  // screen, and until now nothing said so until the save-time on-disk check
  // caught the divergence two steps later.
  await mount();
  await fireEvent.change(document.querySelector("#src") as HTMLSelectElement, {
    target: { value: `${DIR}/core_char_90000002.dat` },
  });
  await fireEvent.click(targetBox(90000001));
  await fireEvent.click(aspect("Window layout"));

  expect(await screen.findByText(/open in the editor/i)).toBeTruthy();

  // And it is about the open file specifically, not about any target.
  await fireEvent.click(targetBox(90000001));
  await fireEvent.click(targetBox(90000003));
  await waitFor(() => expect(screen.queryByText(/open in the editor/i)).toBeNull());
});

test("no write is ever sent on mount", async () => {
  await mount();
  calls.never("setup_apply");
});

test("with a preset as the source, the open character is still a target", async () => {
  // 90000001 is the open document, so it seeds sourcePath on mount and is
  // rightly kept out of its own target list. Switching the source to a preset
  // never cleared sourcePath, so the exclusion outlived its reason and the
  // character you have open could not be written to for the rest of the session.
  calls.stub("settings_preset_list", [
    {
      name: " Layout only ",
      dir: `${DIR}/presets/ Layout only `,
      char_path: `${DIR}/presets/ Layout only /core_char.dat`,
      user_path: `${DIR}/presets/ Layout only /core_user.dat`,
      modified_unix: 0,
      aspects: ["layout"],
      full: false,
      error: null,
    },
  ]);
  await mount();
  expect(() => targetRow(90000001)).toThrow(); // as a character source, excluded

  await fireEvent.click(screen.getByRole("radio", { name: /a preset/i }));
  await fireEvent.change(document.querySelector("#srcpreset") as HTMLSelectElement, {
    target: { value: `${DIR}/presets/Layout only` },
  });

  await waitFor(() => expect(targetRow(90000001)).toBeTruthy());
});

test("a preset source offers only what it holds and sends dir verbatim", async () => {
  // settings_preset_list is unstubbed by default (see `mount`'s comment): the
  // preset value is a lazily-evaluated derived that no other test ever reads.
  // This test is the one place it matters, so it stubs it itself.
  //
  // The name carries a leading AND trailing space on purpose. The claim here is
  // byte-for-byte passthrough, and whitespace is the only thing a stray .trim()
  // would change -- a fixture without it catches a case change or a swapped
  // field, but that one specifically slips past.
  calls.stub("settings_preset_list", [
    {
      name: " Layout only ",
      dir: `${DIR}/presets/ Layout only `,
      char_path: `${DIR}/presets/ Layout only /core_char.dat`,
      user_path: `${DIR}/presets/ Layout only /core_user.dat`,
      modified_unix: 0,
      aspects: ["layout"],
      full: false,
      error: null,
    },
    {
      name: "Broken",
      dir: `${DIR}/presets/Broken`,
      char_path: `${DIR}/presets/Broken/core_char.dat`,
      user_path: `${DIR}/presets/Broken/core_user.dat`,
      modified_unix: 0,
      aspects: [],
      full: false,
      error: "decode failed",
    },
  ]);
  await mount();

  await fireEvent.click(screen.getByRole("radio", { name: /a preset/i }));

  // The broken preset never becomes an option at all.
  const presetSelect = document.querySelector("#srcpreset") as HTMLSelectElement;
  expect([...presetSelect.options].some((o) => o.textContent?.includes("Broken"))).toBe(false);

  await fireEvent.change(presetSelect, { target: { value: `${DIR}/presets/ Layout only ` } });

  // This preset holds only Layout, so Overview must not be offered.
  expect(() => aspect("Overview (columns, tabs, presets)")).toThrow();

  await fireEvent.click(targetBox(90000002));
  await fireEvent.click(aspect("Window layout"));

  const apply = screen.getByRole("button", { name: /apply/i });
  await waitFor(() => expect(apply.hasAttribute("disabled")).toBe(false));
  await fireEvent.click(apply);

  await waitFor(() => expect(calls.of("setup_apply").length).toBe(1));
  const sent = calls.only("setup_apply").args!;
  // Sent byte-for-byte as returned: the backend checks this path case-sensitively.
  expect(sent.source).toEqual({
    kind: "preset",
    dir: `${DIR}/presets/ Layout only `,
    anchor_dir: DIR,
  });
});
