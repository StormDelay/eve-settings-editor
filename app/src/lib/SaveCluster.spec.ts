// One control replacing two dirty badges, a Discard button and a fidelity
// badge — and the disclosure that finally says WHICH files a save writes and
// whose settings they are.
//
// The load-bearing assertion is the last one: the button, the disclosure and the
// save loop all ask `saveable`, so a Save that lights up for a file the loop
// would skip is impossible by construction rather than by review.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import SaveCluster from "$lib/SaveCluster.svelte";
import { accountsStore } from "$lib/accounts.svelte";
import { names } from "$lib/names.svelte";
import { subject } from "$lib/subject.svelte";
import type { OpenOutcome } from "$lib/api";

const opened = (file_name: string, readOnly = false): OpenOutcome => ({
  status: "opened",
  path: `/eve/${file_name}`,
  file_name,
  fidelity: readOnly ? { state: "read_only", reason: "unsupported opcode" } : { state: "editable" },
  tree: {
    label: "root", kind: "dict", display: "{}", path: [],
    editable: false, edit_text: null, removable: false, in_shared: false, children: [],
  },
});

const save = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;

async function disclose() {
  await fireEvent.click(screen.getByRole("button", { name: /unsaved/ }));
  return await screen.findByRole("dialog", { name: "Unsaved changes" });
}

test("a clean subject shows Save disabled and no count", () => {
  subject.slots.char = opened("core_char_950.dat");
  render(SaveCluster);
  expect(save().disabled).toBe(true);
  expect(screen.queryByText(/unsaved/)).toBeNull();
});

test("nothing open still renders Save, disabled, saying to open a character", () => {
  render(SaveCluster);
  expect(save().disabled).toBe(true);
  expect(save().getAttribute("title")).toMatch(/open a character/i);
});

describe("the disclosure", () => {
  test("lists both files with subject, role and file name, and names the siblings", async () => {
    names[950] = { name: "Baguette Commander", category: "character" };
    names[951] = { name: "Clea Otsada", category: "character" };
    accountsStore.roster = {
      accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950, 951] }],
      unassigned: [],
    };
    subject.slots.char = opened("core_char_950.dat");
    subject.slots.user = opened("core_user_140.dat");
    subject.dirty.char = true;
    subject.dirty.user = true;
    render(SaveCluster);

    expect(screen.getByText("2 unsaved ▾")).toBeTruthy();
    const box = await disclose();
    const text = box.textContent ?? "";
    expect(text).toMatch(/Will write/i);
    expect(text).toMatch(/Baguette Commander/);
    expect(text).toMatch(/character/);
    expect(text).toMatch(/core_char_950\.dat/);
    expect(text).toMatch(/stormdelay2/);
    expect(text).toMatch(/account/);
    expect(text).toMatch(/core_user_140\.dat/);
    // The single most consequential fact in the app, stated as a consequence
    // rather than as a storage location.
    expect(text).toMatch(/this also changes Clea Otsada/);
    expect(within_(box, "Discard changes")).toBeTruthy();
  });

  test("only the account file dirty still names the siblings", async () => {
    names[951] = { name: "Clea Otsada", category: "character" };
    accountsStore.roster = {
      accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950, 951] }],
      unassigned: [],
    };
    subject.slots.char = opened("core_char_950.dat");
    subject.slots.user = opened("core_user_140.dat");
    subject.dirty.user = true;
    render(SaveCluster);

    expect(screen.getByText("1 unsaved ▾")).toBeTruthy();
    const text = (await disclose()).textContent ?? "";
    expect(text).toMatch(/core_user_140\.dat/);
    expect(text).not.toMatch(/core_char_950\.dat/);
    expect(text).toMatch(/Clea Otsada/);
  });

  /**
   * A dirty slot that CANNOT be written is still listed — "your edits are stuck
   * in a read-only file" is exactly what the user needs told — but it is not
   * under "will write", and Save agrees with that by staying disabled.
   */
  test("a dirty read-only document is listed with its reason and Save stays disabled", async () => {
    subject.slots.char = opened("core_char_950.dat", true);
    subject.dirty.char = true;
    render(SaveCluster);

    expect(save().disabled).toBe(true);
    const text = (await disclose()).textContent ?? "";
    expect(text).toMatch(/Cannot be written/i);
    expect(text).toMatch(/unsupported opcode/);
    expect(text).not.toMatch(/Will write/i);
  });
});

/** `getByText` scoped to an element, without pulling in `within` for one use. */
function within_(root: HTMLElement, text: string): boolean {
  return (root.textContent ?? "").includes(text);
}
