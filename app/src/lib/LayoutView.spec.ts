// Component test (vitest + jsdom).
//
// LayoutView is the largest component in the app and had no test of any kind.
// The drag/snap/drop DECISIONS live in layout.ts and are covered there; what is
// only observable here is the wiring around them — which document each load
// reads, what a keyboard nudge actually sends and how often, and that read-only
// really means nothing is written.
//
// The nudge is the part worth pinning. Key auto-repeat fires dozens of keydowns
// and exactly one keyup, so "preview on keydown, commit on keyup" is what keeps
// a held arrow from being dozens of writes. Nothing else checks that.
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import LayoutView from "$lib/LayoutView.svelte";
import { calls } from "$lib/test/setup";
import { setClutterOverride, clearClutterOverrides } from "$lib/prefs.svelte";
import type { Hud, HudEntry, WindowLayout, WindowRect } from "$lib/api";

const path = (n: string) => [{ s: "d", i: 1 }, { s: "k", i: 0 }, { s: n }];

function win(id: string, x = 100, y = 200): WindowRect {
  return {
    id,
    label: id,
    name: null,
    open: true,
    renderable: true,
    resolution_matches: true,
    geom: {
      x, y, w: 400, h: 300, screen_w: 2560, screen_h: 1440,
      x_path: path(`${id}.x`), y_path: path(`${id}.y`),
      w_path: path(`${id}.w`), h_path: path(`${id}.h`),
      screen_w_path: path(`${id}.sw`), screen_h_path: path(`${id}.sh`),
    },
    flags: [],
    stack: null,
  } as unknown as WindowRect;
}

const layout = (...windows: WindowRect[]): WindowLayout =>
  ({ reference_w: 2560, reference_h: 1440, windows, stacks: [] }) as WindowLayout;

function mount(over: Record<string, unknown> = {}) {
  const runMutations = vi.fn().mockResolvedValue(undefined);
  render(LayoutView, {
    slot: "char",
    runMutations,
    readOnly: false,
    refreshToken: 1,
    userOpen: false,
    selectedId: null,
    onReveal: vi.fn(),
    onDirty: vi.fn(),
    ...over,
  } as never);
  return { runMutations };
}

/** The mutations of the single `runMutations` call, flattened. */
const sent = (fn: ReturnType<typeof vi.fn>) => fn.mock.calls.flatMap((c) => c[0]);

describe("loading", () => {
  test("reads the window layout for the slot it was given", async () => {
    calls.stub("window_layout", layout(win("overview")));
    mount({ slot: "user" });
    await waitFor(() => expect(calls.only("window_layout").args).toEqual({ slot: "user" }));
  });

  // Furniture, the neocom bar, the overview columns and the chat splits are all
  // bonus layers: an account file opened on its own has none of them, and a
  // failure in any one of them must not take the canvas down.
  test("a canvas still renders when every bonus layer fails", async () => {
    calls.stub("window_layout", layout(win("overview")));
    for (const cmd of ["hud_layout", "neocom_bar", "overview_columns", "chat_panels"]) {
      calls.stub(cmd, () => { throw new Error("no such document"); });
    }
    mount();
    // The id appears in both the window list and the canvas rect, so this is
    // getAll — see test/README.md on scoping repeated labels.
    await waitFor(() => expect(screen.getAllByText(/overview/i).length).toBeGreaterThan(0));
  });
});

describe("arrow-key nudge", () => {
  async function mounted(readOnly = false) {
    calls.stub("window_layout", layout(win("overview")));
    const h = mount({ selectedId: "overview", readOnly });
    await waitFor(() => expect(calls.of("window_layout").length).toBe(1));
    return h;
  }

  // The whole point of previewing on keydown: a held arrow is dozens of
  // keydowns and one keyup, and it must cost exactly one write.
  test("a held arrow commits once, on release", async () => {
    const { runMutations } = await mounted();

    for (let i = 0; i < 12; i++) {
      await fireEvent.keyDown(window, { key: "ArrowRight" });
    }
    expect(runMutations).not.toHaveBeenCalled();

    await fireEvent.keyUp(window, { key: "ArrowRight" });
    await waitFor(() => expect(runMutations).toHaveBeenCalledTimes(1));

    // Twelve presses of one px each, from x=100.
    const xs = sent(runMutations).filter((m) => m.path?.[2]?.s === "overview.x");
    expect(xs).toHaveLength(1);
    expect(xs[0].text).toBe("112");
  });

  test("Shift makes each step ten", async () => {
    const { runMutations } = await mounted();
    await fireEvent.keyDown(window, { key: "ArrowDown", shiftKey: true });
    await fireEvent.keyUp(window, { key: "ArrowDown" });
    await waitFor(() => expect(runMutations).toHaveBeenCalledTimes(1));
    const ys = sent(runMutations).filter((m) => m.path?.[2]?.s === "overview.y");
    expect(ys[0].text).toBe("210"); // 200 + 10
  });

  // Alt is the snap-disable modifier for drags; a nudge never snaps, so
  // Alt+Arrow must do nothing rather than nudging.
  test.each([
    ["Alt", { altKey: true }],
    ["Ctrl", { ctrlKey: true }],
    ["Cmd", { metaKey: true }],
  ])("%s+Arrow writes nothing", async (_name, mods) => {
    const { runMutations } = await mounted();
    await fireEvent.keyDown(window, { key: "ArrowRight", ...mods });
    await fireEvent.keyUp(window, { key: "ArrowRight" });
    expect(runMutations).not.toHaveBeenCalled();
  });

  test("a read-only document is never nudged", async () => {
    const { runMutations } = await mounted(true);
    await fireEvent.keyDown(window, { key: "ArrowRight" });
    await fireEvent.keyUp(window, { key: "ArrowRight" });
    expect(runMutations).not.toHaveBeenCalled();
  });

  // The arrows belong to a focused text field, not to the canvas. A checkbox
  // does NOT use them — the window panel's own filter toggles are checkboxes,
  // and treating every INPUT alike left the nudge dead while one held focus.
  test("a focused text input keeps its arrows", async () => {
    const { runMutations } = await mounted();
    const input = document.createElement("input");
    input.type = "text";
    document.body.appendChild(input);
    await fireEvent.keyDown(input, { key: "ArrowRight", bubbles: true });
    await fireEvent.keyUp(input, { key: "ArrowRight", bubbles: true });
    expect(runMutations).not.toHaveBeenCalled();
  });

  test("a focused checkbox does not", async () => {
    const { runMutations } = await mounted();
    const box = document.createElement("input");
    box.type = "checkbox";
    document.body.appendChild(box);
    await fireEvent.keyDown(box, { key: "ArrowRight", bubbles: true });
    await fireEvent.keyUp(box, { key: "ArrowRight", bubbles: true });
    await waitFor(() => expect(runMutations).toHaveBeenCalledTimes(1));
  });

  // geomMutations diffs per field, so a preview that ended up back on the
  // committed geometry produces an empty mutation list — and `commit` returns
  // before touching the backend at all. A glide out and back must not dirty
  // the document.
  test("a nudge that lands back on the committed value writes nothing", async () => {
    const { runMutations } = await mounted();
    await fireEvent.keyDown(window, { key: "ArrowRight" });
    await fireEvent.keyDown(window, { key: "ArrowLeft" });
    await fireEvent.keyUp(window, { key: "ArrowLeft" });
    expect(runMutations).not.toHaveBeenCalled();
  });

  test("nothing selected, nothing nudged", async () => {
    calls.stub("window_layout", layout(win("overview")));
    const { runMutations } = mount({ selectedId: null });
    await waitFor(() => expect(calls.of("window_layout").length).toBe(1));
    await fireEvent.keyDown(window, { key: "ArrowRight" });
    await fireEvent.keyUp(window, { key: "ArrowRight" });
    expect(runMutations).not.toHaveBeenCalled();
  });
});

/**
 * The DOM contract the shell's grid depends on.
 *
 * This is the one view that fills BOTH shell columns, and it reaches the
 * inspector column without a portal: its root is `display: contents`, so its two
 * children stop participating in its layout and become grid items of `.shell` —
 * landing in columns 2 and 3 through the `.work` / `.inspector` rules in
 * `app.css`.
 *
 * That holds only while the root has EXACTLY those two element children. Wrap
 * them in a scroller, or add a third sibling, and the canvas silently moves into
 * the inspector's column with nothing failing. jsdom computes no layout, so the
 * pixels are checked in the running app — but this is the half that gets broken
 * later by an edit that has nothing to do with the grid.
 */
// The status line carried a fact, a view setting, an instruction and two
// counters with links, in one sentence and five tones. What NARROWS the view is
// two chips now: each appears only when it has something to say, and each
// carries its own escape.
describe("the status bar", () => {
  test("a chip counts what the filter is hiding, and dismissing it resets", async () => {
    calls.stub("window_layout", layout(win("overview"), win("market")));
    mount();
    await waitFor(() => expect(calls.of("window_layout").length).toBe(1));

    const box = (await screen.findByLabelText("Filter windows")) as HTMLInputElement;
    await fireEvent.input(box, { target: { value: "market" } });
    const chip = await screen.findByText(/of 2 windows/);

    // Back to the DEFAULT, which hides clutter — not to nothing.
    await fireEvent.click(within(chip.parentElement as HTMLElement).getByRole("button", { name: "Show every window again" }));
    expect(box.value).toBe("");
  });

  test("a chip counts the clutter overrides, and dismissing it clears them", async () => {
    setClutterOverride("market", "clutter");
    calls.stub("window_layout", layout(win("overview"), win("market")));
    mount();

    const chip = await screen.findByText("1 overridden");
    await fireEvent.click(within(chip.parentElement as HTMLElement).getByRole("button", { name: "Clear the clutter overrides" }));
    await waitFor(() => expect(screen.queryByText("1 overridden")).toBeNull());
  });

  test("no overrides, no chip", async () => {
    clearClutterOverrides(new Set(["overview", "market"]));
    calls.stub("window_layout", layout(win("overview")));
    mount();
    await waitFor(() => expect(calls.of("window_layout").length).toBe(1));
    expect(screen.queryByText(/overridden/)).toBeNull();
  });

  // Instruction, not status: it moved to the pane that has to say something
  // when nothing is selected anyway.
  test("the drag hint is in the inspector, not under the canvas", async () => {
    calls.stub("window_layout", layout(win("overview")));
    mount({ selectedId: null });
    const hint = await screen.findByText(/Shift-drag onto another window to stack/);
    expect(hint.closest(".inspector")).toBeTruthy();
  });

  test("a read-only file is told none of it", async () => {
    calls.stub("window_layout", layout(win("overview")));
    mount({ selectedId: null, readOnly: true });
    await waitFor(() => expect(calls.of("window_layout").length).toBe(1));
    expect(screen.queryByText(/Shift-drag onto another window to stack/)).toBeNull();
  });
});

/**
 * The target list is dragged by its ANCHOR, not by its box.
 *
 * The box hangs off whichever side of the anchor faces the middle of the
 * screen, so a whole-box grab holds the cursor at an offset that changes SIGN
 * the moment the anchor crosses the middle — the box jumps out from under the
 * hand. In game the handle IS the anchor. So is the marker here.
 *
 * jsdom lays nothing out, so "the dot exists" and "the dot got an event" would
 * both pass against something no cursor can reach. What is asserted instead is
 * the difference the change is FOR: same gesture, two origins, and only one of
 * them moves the box or writes anything. The other still selects.
 */
describe("the target list's grab handle", () => {
  // `bind:clientWidth` reports 0 in jsdom, which makes the canvas scale 0 and
  // every drag delta round to zero data px — so a drag would look identical to
  // no drag. Svelte's size binding also reads `element.clientWidth` once,
  // directly, in a mount effect (not just from the ResizeObserver it installs),
  // so a prototype getter is enough to give the canvas a real scale. 2560 for a
  // 2560-wide reference: scale 1, and client px are data px.
  let clientWidth: PropertyDescriptor | undefined;
  beforeEach(() => {
    clientWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 2560 });
  });
  afterEach(() => {
    if (clientWidth) Object.defineProperty(HTMLElement.prototype, "clientWidth", clientWidth);
  });

  // The values Holy Storm's account file held at the 2026-07-31 capture, which
  // put the anchor at 1426, 752 on a 2560x1440 client — past the middle on both
  // axes, so the box hangs up and to the left and the marker is on `br`.
  const hudEntry = (name: string, value: string): HudEntry =>
    ({ name, kind: "float", value, default: "0", scope: "account", set: { how: "set", path: [] } }) as HudEntry;
  const targetHud: Hud = {
    entries: [
      hudEntry("target_x", "0.5442122186495176"),
      hudEntry("target_y", "0.5222222222222223"),
    ],
  };

  /** The one furniture rect this hud draws, and its anchor marker. */
  async function canvas() {
    calls.stub("window_layout", layout(win("overview")));
    calls.stub("hud_layout", targetHud);
    calls.stub("set_hud_value", targetHud);
    mount();
    const box = await waitFor(() => {
      const el = document.querySelector(".furniture");
      expect(el).toBeTruthy();
      return el as HTMLElement;
    });
    return { box, dot: box.querySelector(".anchor-dot") as HTMLElement, surface: document.querySelector(".canvas") as HTMLElement };
  }

  /** Press on `from`, travel 100x40 across the canvas, release. */
  async function drag(from: HTMLElement, surface: HTMLElement) {
    await fireEvent.pointerDown(from, { pointerId: 1, clientX: 400, clientY: 400 });
    await fireEvent.pointerMove(surface, { pointerId: 1, clientX: 500, clientY: 440 });
  }

  test("a press on the box selects it and moves nothing", async () => {
    const { box, surface } = await canvas();
    const before = box.style.left;

    await drag(box, surface);
    expect(box.style.left).toBe(before);

    await fireEvent.pointerUp(surface, { pointerId: 1, clientX: 500, clientY: 440 });
    calls.never("set_hud_value");
    // Selectable, just not draggable — the whole point of not simply making the
    // box inert.
    expect(box.classList.contains("selected")).toBe(true);
  });

  test("a press on the anchor marker drags", async () => {
    const { box, dot, surface } = await canvas();
    const before = box.style.left;

    await drag(dot, surface);
    // The box is placed from the anchor, so moving the anchor moves the box.
    expect(box.style.left).not.toBe(before);

    await fireEvent.pointerUp(surface, { pointerId: 1, clientX: 500, clientY: 440 });
    await waitFor(() => expect(calls.of("set_hud_value").length).toBeGreaterThan(0));
    expect(calls.of("set_hud_value").map((c) => c.args?.name).sort()).toEqual(["target_x", "target_y"]);
    // Still selected: the marker press selects exactly as the box press does.
    expect(box.classList.contains("selected")).toBe(true);
  });

  test("a read-only file drags by neither", async () => {
    calls.stub("window_layout", layout(win("overview")));
    calls.stub("hud_layout", targetHud);
    mount({ readOnly: true });
    const box = await waitFor(() => {
      const el = document.querySelector(".furniture");
      expect(el).toBeTruthy();
      return el as HTMLElement;
    });
    const surface = document.querySelector(".canvas") as HTMLElement;
    const before = box.style.left;
    await drag(box.querySelector(".anchor-dot") as HTMLElement, surface);
    await fireEvent.pointerUp(surface, { pointerId: 1, clientX: 500, clientY: 440 });
    expect(box.style.left).toBe(before);
    calls.never("set_hud_value");
  });
});

describe("the shell grid contract", () => {
  test("the root renders exactly the work area and the inspector, as siblings", async () => {
    calls.stub("window_layout", layout(win("overview")));
    mount({ selectedId: null });
    await waitFor(() => expect(calls.of("window_layout").length).toBe(1));

    const root = await waitFor(() => {
      const el = document.querySelector(".layout-view");
      expect(el).toBeTruthy();
      return el as HTMLElement;
    });
    const kids = Array.from(root.children);
    expect(kids).toHaveLength(2);
    expect(kids[0].classList.contains("work")).toBe(true);
    expect(kids[1].classList.contains("inspector")).toBe(true);
  });

  // The shell's hide control lives in the aside this view replaces, so without
  // one here the column could be reopened from Layout and only closed from
  // another tab. Missing since the shell grew the column.
  test("the inspector can be collapsed from here", async () => {
    calls.stub("window_layout", layout(win("overview")));
    const onCollapseInspector = vi.fn();
    mount({ onCollapseInspector });
    await fireEvent.click(await screen.findByRole("button", { name: "Hide properties" }));
    expect(onCollapseInspector).toHaveBeenCalled();
  });
});
