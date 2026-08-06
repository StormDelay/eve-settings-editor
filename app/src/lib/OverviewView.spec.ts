// Component test (vitest + jsdom).
//
// OverviewView is the shell around the three overview sub-tabs. What it owns
// alone is the gating: overview columns live in the ACCOUNT file, so a
// character with no account paired can see nothing here and must be told why
// rather than shown an empty editor. It also owns tab selection, and the rule
// that a reload keeps the selected tab only when that tab still exists.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import OverviewView from "$lib/OverviewView.svelte";
import { calls } from "$lib/test/setup";
import type { Appearance, OverviewColumns, OverviewTab } from "$lib/api";

const tab = (index: number, name: string): OverviewTab => ({
  index,
  name,
  // A real tab always names a preset; the sub-tabs read it and an empty string
  // is not the same thing as "no preset" to them.
  preset: "PvP",
  inherits: false,
  columns: [{ name: "NAME", label: "Name", visible: true, width: 120 }],
});

const appearance: Appearance = {
  background: { enabled: [], order: [] },
  flag: { enabled: [], order: [] },
  colors: [],
  bools: [],
  defaulted: false,
};

const columns = (...tabs: OverviewTab[]): OverviewColumns => ({
  tabs,
  windows: [],
  presets: [{ name: "PvP", groups: [], filtered_states: [], always_shown_states: [] }],
  appearance,
});

function mount(over: Record<string, unknown> = {}) {
  const spies = {
    onLoadCharacter: vi.fn(), onUserDirty: vi.fn(), onCharDirty: vi.fn(),
    onWindowAdded: vi.fn(), onShowAccounts: vi.fn(),
  };
  render(OverviewView, {
    userOpen: true,
    userId: 5,
    charId: 9,
    charOpen: true,
    characters: [9],
    refreshToken: 1,
    ...spies,
    ...over,
  } as never);
  return spies;
}

describe("gating on the account file", () => {
  // The columns live in the account file. A character with none paired is not
  // an error state — it is the normal state before pairing — so it gets the
  // route out rather than an empty editor.
  test("an unpaired character is offered the pairing flow", async () => {
    const { onShowAccounts } = mount({ userOpen: false, charId: 9 });
    const button = await screen.findByRole("button", { name: /pair/i });
    await fireEvent.click(button);
    expect(onShowAccounts).toHaveBeenCalled();
    calls.never("overview_columns");
  });

  test("with no file open at all, nothing is read", async () => {
    mount({ userOpen: false, charId: null });
    expect(screen.getByText(/open a character or account file/i)).toBeTruthy();
    calls.never("overview_columns");
  });

  test("an open account file is read once", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP")));
    mount();
    await waitFor(() => expect(calls.of("overview_columns").length).toBe(1));
  });

  test("a backend failure is shown, not swallowed", async () => {
    calls.stub("overview_columns", () => { throw { code: "no_document", message: "no account file open" }; });
    mount();
    expect(await screen.findByText(/no account file open/i)).toBeTruthy();
  });

  test("an account with no tabs says so", async () => {
    calls.stub("overview_columns", columns());
    mount();
    expect(await screen.findByText(/no overview tabs/i)).toBeTruthy();
  });
});

describe("tab selection", () => {
  test("the first tab is selected on load", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP"), tab(1, "Mining")));
    mount();
    const select = (await screen.findByLabelText("Tab")) as HTMLSelectElement;
    expect(select.value).toBe("0");
  });

  test("every tab is offered", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP"), tab(1, "Mining")));
    mount();
    await screen.findByLabelText("Tab");
    expect(screen.getByRole("option", { name: "PvP" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Mining" })).toBeTruthy();
  });

  // Switching between two account files leaves userOpen/userId unchanged, so
  // refreshToken is what has to drive the reload — otherwise the second file
  // shows the first one's overview.
  test("a refreshToken bump re-reads the file", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP")));
    const { rerender } = render(OverviewView, {
      userOpen: true, userId: 5, charId: 9, charOpen: true, characters: [9], refreshToken: 1,
      onLoadCharacter: vi.fn(), onUserDirty: vi.fn(), onCharDirty: vi.fn(),
      onWindowAdded: vi.fn(), onShowAccounts: vi.fn(),
    } as never);
    await waitFor(() => expect(calls.of("overview_columns").length).toBe(1));

    await rerender({ refreshToken: 2 } as never);
    await waitFor(() => expect(calls.of("overview_columns").length).toBe(2));
  });
});
