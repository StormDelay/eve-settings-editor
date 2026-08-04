// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test, vi, beforeEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import ProbeFormationsView from "$lib/ProbeFormationsView.svelte";
import { calls } from "$lib/test/setup";
import { message, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { Formation, Formations, FormationSpec } from "$lib/api";

// The view raises dialogs on every failure path; jsdom has no Tauri to answer
// them, and a test asserting on WHICH message appeared needs the spy anyway.
vi.mock("@tauri-apps/plugin-dialog", () => ({
  message: vi.fn(() => Promise.resolve()),
  open: vi.fn(),
  save: vi.fn(),
}));

const noop = () => {};

// A coordinate with bits well below what any rounded AU display can carry.
const AWKWARD: [number, number, number] = [-1199120384.7, -115136512.3, -415997952.9];

/** A minimal shared document, for every test that needs some text to paste. */
const SHARED = "formations:\n  - name: close\n    range: 74798935350\n    probes:\n      - [1, 0, 0]\n";

const FORMATIONS: Formations = {
  formations: [
    {
      id: 0,
      name: "close",
      probes: [AWKWARD, [1e9, 2e9, 3e9]],
      ranges: [74798935350, 74798935350],
    },
  ],
  selected: 0,
};

async function open() {
  calls.stub("probe_formations", FORMATIONS);
  calls.stub("set_probe_formation", FORMATIONS);
  render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
  await screen.findByDisplayValue("close");
}

/** The arguments of the last set_probe_formation call. */
const lastSet = () => {
  const c = [...calls.log].reverse().find((x) => x.cmd === "set_probe_formation");
  return c?.args as { id: number | null; name: string; probes: number[][]; ranges: number[] };
};

describe("precision", () => {
  test("an untouched coordinate is sent back to the metre", async () => {
    // One metre is 6.7e-12 AU. If a displayed, rounded AU string were the
    // source of truth, saving after editing ANY field would displace every
    // other probe in the formation — silently, and on every save.
    await open();
    const nameField = await screen.findByDisplayValue("close");
    await fireEvent.input(nameField, { target: { value: "closer" } });
    await fireEvent.blur(nameField);

    const args = lastSet();
    expect(args.name).toBe("closer");
    expect(args.probes[0]).toEqual(AWKWARD);
  });
});

describe("editing", () => {
  test("typing a distance moves the probe along its existing direction", async () => {
    await open();
    // Probe 1's distance field, doubled. Its angles must not change, so the
    // new position is the old one scaled by two.
    const dist = await screen.findByLabelText("probe 1 distance");
    const before = AWKWARD;
    const r = Math.hypot(...before);
    await fireEvent.input(dist, { target: { value: String((r * 2) / 149597870700) } });
    await fireEvent.blur(dist);

    const p = lastSet().probes[0];
    for (let i = 0; i < 3; i++) expect(p[i]).toBeCloseTo(before[i] * 2, 0);
    // The scenario the "untouched coordinate" test's comment describes: edit
    // ONE probe, and a DIFFERENT one must not move at all.
    expect(lastSet().probes[1]).toEqual([1e9, 2e9, 3e9]);
  });

  test("blurring a field without changing it does not commit", async () => {
    // The blur handler used to commit unconditionally, so tabbing through a
    // formation without editing anything still wrote it back to the file and
    // lit the "unsaved" badge.
    await open();
    const nameField = await screen.findByDisplayValue("close");
    await fireEvent.focus(nameField);
    await fireEvent.blur(nameField);

    calls.never("set_probe_formation");
  });

  test("range offers EVE's slider stops and nothing else", async () => {
    // In-game the scan range is a slider with fixed stops, so a free-text field
    // could write a range the client has no way to represent. A picker also
    // makes a zero-or-negative range — meaningless in EVE, and an invalid SVG
    // radius — unreachable by construction.
    await open();
    const range = (await screen.findByLabelText("range for every probe")) as HTMLSelectElement;
    const offered = [...range.options].map((o) => o.text);
    expect(offered).toEqual([
      "0.25 AU", "0.5 AU", "1 AU", "2 AU", "4 AU", "8 AU", "16 AU", "32 AU", "64 AU",
    ]);
  });

  test("choosing a range sends that stop's metres", async () => {
    await open();
    const range = await screen.findByLabelText("range for every probe");
    await fireEvent.change(range, { target: { value: String(149597870700) } });
    expect(lastSet().ranges).toEqual([149597870700, 149597870700]);
  });

  test("New selects the newly minted formation even when its id fills a gap", async () => {
    // next_id fills the lowest free gap (probes.rs), so with ids {0, 2} the
    // new formation lands at id 1 — the MIDDLE of the sorted response, not
    // its end. Selecting by position would land on id 2 ("b") instead.
    const a: Formation = { id: 0, name: "a", probes: [[1, 2, 3]], ranges: [74798935350] };
    const b: Formation = { id: 2, name: "b", probes: [[4, 5, 6]], ranges: [74798935350] };
    const created: Formation = { id: 1, name: "New formation", probes: [[0, 0, 0]], ranges: [74798935350] };
    calls.stub("probe_formations", { formations: [a, b], selected: 0 } satisfies Formations);
    calls.stub("set_probe_formation", { formations: [a, created, b], selected: 1 } satisfies Formations);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
    await screen.findByDisplayValue("a");

    await fireEvent.click(screen.getByText("New"));

    expect(await screen.findByDisplayValue("New formation")).toBeTruthy();
  });

  test("switching account resyncs the drafts even when the id stays valid", async () => {
    // The corpus puts formations at ids 0 and 1, so switching accounts normally
    // leaves `selectedId` pointing at something that still exists — and the
    // re-select that resyncs the buffer only fired when it had vanished. The
    // Name field then showed account A's formation over account B's list, and a
    // blur with no typing at all wrote A's formation into B's file.
    calls.stub("probe_formations", {
      formations: [{ id: 0, name: "close", probes: [[111, 0, 0]], ranges: [74798935350] }],
      selected: 0,
    } satisfies Formations);
    calls.stub("set_probe_formation", FORMATIONS);
    const { rerender } = render(ProbeFormationsView, {
      userOpen: true, userId: 1, onUserDirty: noop,
    });
    await screen.findByDisplayValue("close");

    calls.stub("probe_formations", {
      formations: [{ id: 0, name: "grid", probes: [[999, 0, 0]], ranges: [74798935350] }],
      selected: 0,
    } satisfies Formations);
    await rerender({ userOpen: true, userId: 2, onUserDirty: noop });

    const name = await screen.findByDisplayValue("grid");
    await fireEvent.focus(name);
    await fireEvent.blur(name);
    calls.never("set_probe_formation");
  });
});

describe("per-probe range", () => {
  test("a probe's range picker sends only that probe's new range", async () => {
    // The client sets scan range per probe. A picker per row is the whole
    // point of dropping the old single field, so the other rows must not move.
    await open();
    const row = (await screen.findByLabelText("probe 2 range")) as HTMLSelectElement;
    await fireEvent.change(row, { target: { value: String(149597870700) } });

    expect(lastSet().ranges).toEqual([74798935350, 149597870700]);
  });

  test("the header picker sets every probe's range at once", async () => {
    // Uniform range is still the common case; reaching it by setting eight
    // selects by hand would be a regression on the field this replaces.
    await open();
    const all = await screen.findByLabelText("range for every probe");
    await fireEvent.change(all, { target: { value: String(149597870700) } });

    expect(lastSet().ranges).toEqual([149597870700, 149597870700]);
  });

  test("a formation with differing ranges is editable, not locked read-only", async () => {
    // This inverts the old mixed_range behaviour. That flag guarded against
    // flattening a mix through a single range field; with a field per row
    // there is nothing to flatten (spec §2.1, §5.1).
    calls.stub("probe_formations", {
      formations: [{ ...FORMATIONS.formations[0], ranges: [74798935350, 37399467675] }],
      selected: 0,
    });
    calls.stub("set_probe_formation", FORMATIONS);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });

    const nameField = await screen.findByDisplayValue("close");
    expect((nameField as HTMLInputElement).disabled).toBe(false);
    const row = (await screen.findByLabelText("probe 2 range")) as HTMLSelectElement;
    expect(row.disabled).toBe(false);
    expect(row.value).toBe(String(37399467675));
  });

  test("a range the slider cannot produce is shown on its row, not snapped", async () => {
    const odd = 12345678;
    calls.stub("probe_formations", {
      formations: [{ ...FORMATIONS.formations[0], ranges: [odd, 74798935350] }],
      selected: 0,
    });
    calls.stub("set_probe_formation", FORMATIONS);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });

    const row = (await screen.findByLabelText("probe 1 range")) as HTMLSelectElement;
    expect(row.value).toBe(String(odd));
    expect(row.selectedOptions[0].text).toMatch(/not a slider stop/);
  });
});

describe("clipboard sharing", () => {
  /** What writeText was last handed, and what readText will answer. */
  let written: string[] = [];
  let readable: string | Error = "";

  beforeEach(() => {
    written = [];
    readable = "";
    // jsdom implements no clipboard at all, so there is nothing to spy on —
    // define one. `configurable` so each test can redefine it.
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: (t: string) => { written.push(t); return Promise.resolve(); },
        readText: () => (readable instanceof Error
          ? Promise.reject(readable)
          : Promise.resolve(readable)),
      },
    });
  });

  test("Copy sends the draft, not the saved projection", async () => {
    // The whole reason Copy passes data rather than an id: what the user sees
    // is the draft, and blur-commit is async (spec §5.1).
    await open();
    calls.stub("probe_yaml", SHARED);
    const x = await screen.findByLabelText("probe 1 X");
    await fireEvent.input(x, { target: { value: "999" } });

    await fireEvent.click(screen.getByText("Copy"));

    const sent = calls.of("probe_yaml").at(-1)?.args as { formations: FormationSpec[] };
    expect(sent.formations).toHaveLength(1);
    // 999 AU in metres, from the un-blurred field.
    expect(sent.formations[0].probes[0][0]).toBeCloseTo(999 * 149597870700, 0);
    expect(written).toEqual([SHARED]);
  });

  test("Ctrl-C copies the formation, but not from inside a field", async () => {
    await open();
    calls.stub("probe_yaml", SHARED);

    const x = await screen.findByLabelText("probe 1 X");
    await fireEvent.keyDown(x, { key: "c", ctrlKey: true });
    expect(calls.of("probe_yaml")).toHaveLength(0);

    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    await vi.waitFor(() => expect(calls.of("probe_yaml")).toHaveLength(1));
  });

  test("Paste parses the clipboard and adds what it found", async () => {
    await open();
    readable = SHARED;
    calls.stub("probe_parse_yaml", [
      { name: "close", probes: [[1, 0, 0]], ranges: [74798935350] },
    ] satisfies FormationSpec[]);
    calls.stub("add_probe_formations", FORMATIONS);

    await fireEvent.click(screen.getByText("Paste"));

    await vi.waitFor(() => expect(calls.of("add_probe_formations")).toHaveLength(1));
    const sent = calls.of("add_probe_formations")[0].args as { formations: FormationSpec[] };
    expect(sent.formations[0].name).toBe("close");
    // Never set_probe_formation: the collision rule lives in the batch command.
    expect(calls.of("set_probe_formation")).toHaveLength(0);
  });

  test("a paste that parses to no formations says so", async () => {
    // Import has its own message for an empty-but-valid file; without one here
    // the paste is indistinguishable from a button that does nothing.
    await open();
    readable = "formations: []\n";
    calls.stub("probe_parse_yaml", [] satisfies FormationSpec[]);

    await fireEvent.click(screen.getByText("Paste"));

    await vi.waitFor(() => expect(vi.mocked(message)).toHaveBeenCalled());
    expect(vi.mocked(message).mock.calls[0][0]).toMatch(/no formations/);
    calls.never("add_probe_formations");
  });

  test("a refused clipboard read does not fail silently", async () => {
    await open();
    readable = new Error("denied");
    await fireEvent.click(screen.getByText("Paste"));
    await vi.waitFor(() => expect(vi.mocked(message)).toHaveBeenCalled());
    expect(vi.mocked(message).mock.calls[0][0]).toMatch(/Ctrl\+V/);
    calls.never("probe_parse_yaml");
  });

  test("a paste event adds formations without touching the clipboard API", async () => {
    // The Ctrl-V fallback: the keypress IS the permission grant, so this path
    // must work even when readText is refused outright (spec §5.4).
    await open();
    readable = new Error("denied");
    calls.stub("probe_parse_yaml", [
      { name: "close", probes: [[1, 0, 0]], ranges: [74798935350] },
    ] satisfies FormationSpec[]);
    calls.stub("add_probe_formations", FORMATIONS);

    // fireEvent cannot attach clipboardData to a jsdom Event, so build it.
    const ev = new Event("paste", { bubbles: true });
    Object.defineProperty(ev, "clipboardData", { value: { getData: () => SHARED } });
    window.dispatchEvent(ev);

    await vi.waitFor(() => expect(calls.of("add_probe_formations")).toHaveLength(1));
  });

  test("Ctrl-C with a text selection copies the selection, not the formation", async () => {
    // A formation copy is the fallback for when nothing is selected, never an
    // override of a real selection — the hint paragraph or the shared-account
    // banner must still copy normally.
    await open();
    calls.stub("probe_yaml", SHARED);
    const range = document.createRange();
    range.selectNodeContents(document.body);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);

    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });

    calls.never("probe_yaml");
    sel?.removeAllRanges();
  });

  test("a paste event does nothing when the account file isn't open", async () => {
    // The window listeners outlive the markup's `{#if}` branches, so they are
    // live on the "pair this character" hint screen too, where there is no
    // Paste button on screen and nothing loaded to add to.
    render(ProbeFormationsView, { userOpen: false, userId: 1, onUserDirty: noop });

    const ev = new Event("paste", { bubbles: true });
    Object.defineProperty(ev, "clipboardData", { value: { getData: () => SHARED } });
    window.dispatchEvent(ev);

    calls.never("probe_parse_yaml");
  });
});

describe("file sharing", () => {
  test("Export writes the formations that were ticked, drafts included", async () => {
    await open();
    vi.mocked(saveDialog).mockResolvedValueOnce("C:/tmp/formations.yaml");
    const name = await screen.findByDisplayValue("close");
    await fireEvent.input(name, { target: { value: "closer" } });

    await fireEvent.click(screen.getByText("Export…"));
    await screen.findByText("Export 1");
    await fireEvent.click(screen.getByText("Export 1"));

    await vi.waitFor(() => expect(calls.of("probe_export")).toHaveLength(1));
    const sent = calls.of("probe_export")[0].args as { path: string; formations: FormationSpec[] };
    expect(sent.path).toBe("C:/tmp/formations.yaml");
    expect(sent.formations.map((f) => f.name)).toEqual(["closer"]);
  });

  test("cancelling the save dialog opens no picker and writes nothing", async () => {
    await open();
    vi.mocked(saveDialog).mockResolvedValueOnce(null);
    await fireEvent.click(screen.getByText("Export…"));
    await vi.waitFor(() => expect(vi.mocked(saveDialog)).toHaveBeenCalled());
    expect(screen.queryByTestId("picker-backdrop")).toBeNull();
    calls.never("probe_export");
  });

  test("Import adds only the formations that were ticked", async () => {
    await open();
    vi.mocked(openDialog).mockResolvedValueOnce("C:/tmp/fleet.yaml");
    calls.stub("probe_import", [
      { name: "a", probes: [[1, 0, 0]], ranges: [74798935350] },
      { name: "b", probes: [[2, 0, 0]], ranges: [74798935350] },
    ] satisfies FormationSpec[]);
    calls.stub("add_probe_formations", FORMATIONS);

    await fireEvent.click(screen.getByText("Import…"));
    await screen.findByText("Import 2");
    await fireEvent.click(screen.getByLabelText("b"));
    await fireEvent.click(screen.getByText("Import 1"));

    await vi.waitFor(() => expect(calls.of("add_probe_formations")).toHaveLength(1));
    const sent = calls.of("add_probe_formations")[0].args as { formations: FormationSpec[] };
    expect(sent.formations.map((f) => f.name)).toEqual(["a"]);
  });

  test("a paste behind the open picker is ignored", async () => {
    // The overlay is CSS, not a real modal, so window events still land on the
    // view's listeners. A paste here would add formations the picker's `items`
    // snapshot does not hold, and the Export that follows would write a set one
    // paste out of date.
    await open();
    vi.mocked(saveDialog).mockResolvedValueOnce("C:/tmp/formations.yaml");
    await fireEvent.click(screen.getByText("Export…"));
    await screen.findByText("Export 1");

    const ev = new Event("paste", { bubbles: true });
    Object.defineProperty(ev, "clipboardData", { value: { getData: () => SHARED } });
    window.dispatchEvent(ev);

    calls.never("probe_parse_yaml");
    calls.never("add_probe_formations");
  });

  test("an unreadable file is reported and opens no picker", async () => {
    await open();
    vi.mocked(openDialog).mockResolvedValueOnce("C:/tmp/overview.yaml");
    calls.stub("probe_import", () => {
      throw { code: "not_formations", message: "This file contains no probe formations." };
    });

    await fireEvent.click(screen.getByText("Import…"));

    await vi.waitFor(() => expect(vi.mocked(message)).toHaveBeenCalled());
    expect(screen.queryByTestId("picker-backdrop")).toBeNull();
    calls.never("add_probe_formations");
  });
});
