// Component test: vitest + jsdom.
import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import ListRow from "./ListRow.svelte";
import { text } from "./snippet";
import "$lib/test/setup";

describe("ListRow", () => {
  // A real <button> is what makes the row keyboard-operable at all. jsdom does
  // not synthesise the browser's Enter/Space activation, so the element type is
  // the thing worth pinning — it is what the guarantee rests on.
  test("with onclick the label is a real button, not a clickable div", async () => {
    const onclick = vi.fn();
    render(ListRow, { onclick, children: text("core_user_.dat") });

    const label = screen.getByRole("button", { name: "core_user_.dat" });
    expect(label.tagName).toBe("BUTTON");

    await fireEvent.click(label);
    expect(onclick).toHaveBeenCalledTimes(1);
  });

  test("without onclick there is no button to tab to", () => {
    render(ListRow, { children: text("core_user_.dat") });
    expect(screen.queryByRole("button")).toBeNull();
  });

  test("selected is announced, and unselected rows say so too", () => {
    const { unmount } = render(ListRow, { selected: true, children: text("Astra") });
    expect(screen.getByRole("option", { selected: true })).toBeTruthy();
    unmount();

    render(ListRow, { selected: false, children: text("Astra") });
    expect(screen.getByRole("option", { selected: false })).toBeTruthy();
  });

  // A row that is not part of a selectable set should not claim to be one.
  test("a row with no selection state carries no option role", () => {
    const { container } = render(ListRow, { children: text("Astra") });
    expect(container.querySelector("[role='option']")).toBeNull();
  });

  test("all four drag handlers forward", async () => {
    const h = {
      ondragstart: vi.fn(),
      ondragover: vi.fn(),
      ondrop: vi.fn(),
      ondragend: vi.fn(),
    };
    const { container } = render(ListRow, { draggable: true, ...h, children: text("Row") });

    const row = container.querySelector(".row")!;
    expect(row.getAttribute("draggable")).toBe("true");

    await fireEvent.dragStart(row);
    await fireEvent.dragOver(row);
    await fireEvent.drop(row);
    await fireEvent.dragEnd(row);

    for (const fn of Object.values(h)) expect(fn).toHaveBeenCalledTimes(1);
  });

  test("the grip is decoration, and is hidden from the accessibility tree", () => {
    const { container } = render(ListRow, { draggable: true, children: text("Row") });
    expect(container.querySelector(".grip")?.getAttribute("aria-hidden")).toBe("true");
  });

  test("right-click forwards without a visible control being added", async () => {
    const oncontextmenu = vi.fn();
    const { container } = render(ListRow, { oncontextmenu, children: text("Row") });

    await fireEvent.contextMenu(container.querySelector(".row")!);

    expect(oncontextmenu).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: "More actions" })).toBeNull();
  });

  test("actions render a visible overflow control that opens the menu", async () => {
    const run = vi.fn();
    render(ListRow, { actions: [{ label: "Copy id", run }], children: text("Row") });

    await fireEvent.click(screen.getByRole("button", { name: "More actions" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Copy id" }));

    expect(run).toHaveBeenCalledTimes(1);
  });
});
