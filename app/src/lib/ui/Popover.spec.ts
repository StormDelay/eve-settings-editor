// Component test: vitest + jsdom.
import { afterEach, describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import Popover from "./Popover.svelte";
import { text } from "./snippet";
import "$lib/test/setup";

// jsdom gives every element a zero-sized rect, so the clamp has nothing to work
// with unless we say how big the popover is. This is the ContextMenu case that
// has never had a test: open near the right or bottom edge and the menu would
// render partly offscreen, potentially clipping the only route to an action.
const size = (w: number, h: number) =>
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
    width: w,
    height: h,
    top: 0,
    left: 0,
    right: w,
    bottom: h,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect);

const viewport = (w: number, h: number) => {
  Object.defineProperty(window, "innerWidth", { value: w, writable: true, configurable: true });
  Object.defineProperty(window, "innerHeight", { value: h, writable: true, configurable: true });
};

afterEach(() => vi.restoreAllMocks());

// These renders use the explicit `{ props: … }` form, unlike every other spec
// here. `anchor` is also the name of a Svelte mount option, so testing-library
// reads a bare object containing it as options rather than props and rejects
// the rest as unknown.

describe("Popover", () => {
  test("opens at the point it was given when there is room", async () => {
    size(200, 100);
    viewport(1000, 800);
    render(Popover, { props: {
      anchor: { x: 120, y: 240 },
      onclose: () => {},
      ariaLabel: "Actions",
      children: text("body"),
    } });

    const el = screen.getByRole("dialog", { name: "Actions" });
    expect(el.style.left).toBe("120px");
    expect(el.style.top).toBe("240px");
  });

  test("clamps back inside the viewport at the right and bottom edges", async () => {
    size(200, 100);
    viewport(1000, 800);
    render(Popover, { props: {
      anchor: { x: 950, y: 780 },
      onclose: () => {},
      ariaLabel: "Actions",
      children: text("body"),
    } });

    const el = screen.getByRole("dialog", { name: "Actions" });
    expect(el.style.left).toBe("800px"); // 1000 - 200
    expect(el.style.top).toBe("700px"); // 800 - 100
  });

  test("never clamps to a negative position when it is larger than the viewport", async () => {
    size(400, 900);
    viewport(300, 500);
    render(Popover, { props: {
      anchor: { x: 10, y: 10 },
      onclose: () => {},
      ariaLabel: "Actions",
      children: text("body"),
    } });

    const el = screen.getByRole("dialog", { name: "Actions" });
    expect(el.style.left).toBe("0px");
    expect(el.style.top).toBe("0px");
  });

  test("Escape closes it", async () => {
    const onclose = vi.fn();
    render(Popover, { props: { anchor: { x: 0, y: 0 }, onclose, ariaLabel: "Actions", children: text("body") } });

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(onclose).toHaveBeenCalledTimes(1);
  });

  test("a pointerdown outside closes it, one inside does not", async () => {
    const onclose = vi.fn();
    render(Popover, { props: { anchor: { x: 0, y: 0 }, onclose, ariaLabel: "Actions", children: text("body") } });

    await fireEvent.pointerDown(screen.getByRole("dialog", { name: "Actions" }));
    expect(onclose).not.toHaveBeenCalled();

    await fireEvent.pointerDown(document.body);
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  test("closed means nothing in the document, and no stray window handlers firing", async () => {
    const onclose = vi.fn();
    render(Popover, { props: {
      anchor: { x: 0, y: 0 },
      open: false,
      onclose,
      ariaLabel: "Actions",
      children: text("body"),
    } });

    expect(screen.queryByRole("dialog")).toBeNull();

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onclose).not.toHaveBeenCalled();
  });
});
