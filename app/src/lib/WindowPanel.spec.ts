// Component test (vitest + jsdom).
//
// WindowPanel is presentational — it owns no backend calls — so what is worth
// pinning is the arithmetic it does on its way to the screen: the per-field
// geometry diff that decides whether an edit is a write at all, and the guards
// that stop a read-only document being edited through a control that still
// renders.
//
// Rows are found by the name button's `title`, which carries the RAW window id.
// The visible text is the display name ("Overview" for `overview`), and those
// repeat across families — see test/README.md on scoping.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
import WindowPanel from "$lib/WindowPanel.svelte";
import type { WindowRect } from "$lib/api";

const path = (n: string) => [{ s: "d", i: 1 }, { s: n }];

function win(id: string, over: Partial<WindowRect> = {}): WindowRect {
  return {
    id,
    label: id,
    name: null,
    open: true,
    renderable: true,
    resolution_matches: true,
    geom: {
      x: 100, y: 200, w: 400, h: 300, screen_w: 2560, screen_h: 1440,
      x_path: path(`${id}.x`), y_path: path(`${id}.y`),
      w_path: path(`${id}.w`), h_path: path(`${id}.h`),
      screen_w_path: path(`${id}.sw`), screen_h_path: path(`${id}.sh`),
    },
    flags: [],
    stack: null,
    ...over,
  } as unknown as WindowRect;
}

function mount(windows: WindowRect[], over: Record<string, unknown> = {}) {
  const spies = {
    onSelect: vi.fn(), onToggleOpen: vi.fn(), onGeom: vi.fn(), onFlag: vi.fn(),
    onReveal: vi.fn(), onUnstack: vi.fn(), onReorder: vi.fn(), onAddToStack: vi.fn(),
    onCreateStack: vi.fn(), onDeleteOrphans: vi.fn(), onClutterOverride: vi.fn(),
    onSetChatSplits: vi.fn(),
  };
  render(WindowPanel, {
    windows,
    stacks: [],
    selectedId: null,
    readOnly: false,
    overrides: { clutter: new Set<string>(), visible: new Set<string>() },
    chats: [],
    accountReadOnly: false,
    userOpen: false,
    sharedNames: [],
    // `filter` is deliberately NOT passed: it is a `$bindable` whose fallback
    // the component creates itself, so typing in the box mutates a real state
    // proxy. A plain object handed in from here would be mutated but never
    // observed — in the app LayoutView passes its own `$state` with `bind:`.
    ...spies,
    ...over,
  } as never);
  return spies;
}

/** One window's row, located by its raw id. */
const row = (id: string) => screen.getByTitle(id).closest(".row") as HTMLElement;

/** The x/y/w/h inputs of a row, in COORDS order. Only the selected row has any. */
const coords = (id: string) =>
  within(row(id)).getAllByRole("spinbutton") as HTMLInputElement[];

describe("the window filter", () => {
  test("narrows the list to matching windows", async () => {
    mount([win("overview"), win("market"), win("chatchannel_local")]);
    expect(screen.queryByTitle("market")).toBeTruthy();

    await fireEvent.input(screen.getByLabelText("Filter windows"), {
      target: { value: "chat" },
    });

    expect(screen.queryByTitle("market")).toBeNull();
    expect(screen.queryByTitle("chatchannel_local")).toBeTruthy();
  });

  test("matching is case-insensitive", async () => {
    mount([win("overview"), win("market")]);
    await fireEvent.input(screen.getByLabelText("Filter windows"), {
      target: { value: "MARKET" },
    });
    expect(screen.queryByTitle("market")).toBeTruthy();
    expect(screen.queryByTitle("overview")).toBeNull();
  });

  test("a filter matching nothing empties the list rather than showing everything", async () => {
    mount([win("overview"), win("market")]);
    await fireEvent.input(screen.getByLabelText("Filter windows"), {
      target: { value: "zzzznothing" },
    });
    expect(screen.queryByTitle("overview")).toBeNull();
    expect(screen.queryByTitle("market")).toBeNull();
  });
});

describe("geometry editing", () => {
  test("the four coordinates show the window's committed geometry", () => {
    mount([win("overview")], { selectedId: "overview" });
    expect(coords("overview").map((i) => i.value)).toEqual(["100", "200", "400", "300"]);
  });

  test("committing a changed number reports the new value", async () => {
    const { onGeom } = mount([win("overview")], { selectedId: "overview" });
    await fireEvent.change(coords("overview")[0], { target: { value: "150" } });

    expect(onGeom).toHaveBeenCalledTimes(1);
    const [w, field, value] = onGeom.mock.calls[0];
    expect(w.id).toBe("overview");
    expect(field).toBe("x");
    expect(value).toBe(150);
  });

  test("each field reports under its own name", async () => {
    const { onGeom } = mount([win("overview")], { selectedId: "overview" });
    const [, , , h] = coords("overview");
    await fireEvent.change(h, { target: { value: "999" } });
    expect(onGeom.mock.calls[0][1]).toBe("h");
    expect(onGeom.mock.calls[0][2]).toBe(999);
  });

  // The panel reports every numeric commit and does not diff — the per-field
  // "did this actually change" test is `geomMutations` in LayoutView, which is
  // what turns a report into a write. What the panel DOES guard is a value it
  // cannot parse.
  test.each([["blank", ""], ["non-numeric", "abc"]])(
    "a %s entry reports nothing",
    async (_label, value) => {
      const { onGeom } = mount([win("overview")], { selectedId: "overview" });
      await fireEvent.change(coords("overview")[0], { target: { value } });
      expect(onGeom).not.toHaveBeenCalled();
    },
  );

  // …and it puts the committed value back, so the box never sits showing a
  // number the document does not hold.
  test("an unparseable entry snaps back to the stored value", async () => {
    mount([win("overview")], { selectedId: "overview" });
    const x = coords("overview")[0];
    await fireEvent.change(x, { target: { value: "abc" } });
    expect(x.value).toBe("100");
  });

  test("a read-only document offers no editable geometry", () => {
    mount([win("overview")], { selectedId: "overview", readOnly: true });
    for (const input of coords("overview")) expect(input.disabled).toBe(true);
  });
});

describe("selection", () => {
  test("clicking a window reports its id", async () => {
    const { onSelect } = mount([win("overview"), win("market")]);
    await fireEvent.click(screen.getByTitle("market"));
    expect(onSelect).toHaveBeenCalledWith("market");
  });

  test("only the selected row shows its geometry", () => {
    mount([win("overview"), win("market")], { selectedId: "overview" });
    expect(coords("overview")).toHaveLength(4);
    expect(within(row("market")).queryAllByRole("spinbutton")).toHaveLength(0);
  });
});
