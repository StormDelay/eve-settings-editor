// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// With no account file open the view is nothing but a hint, and which hint is
// the whole information content: "pair this character" is actionable, "open a
// character" is wrong advice when one is already open. Neither case reaches the
// backend — `reload()` returns early when `userOpen` is false — so no stubs.
import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/svelte";
import AutofillView from "$lib/AutofillView.svelte";

const props = (over: Record<string, unknown> = {}) => ({
  userOpen: false,
  onUserDirty: () => {},
  ...over,
});

describe("the hint shown with no account file open", () => {
  test("an open character is offered pairing even before its name resolves", () => {
    render(AutofillView, props({ charOpen: true, charName: null }));
    expect(screen.getByText(/remembered text lives/i).textContent).toContain("This character");
    expect(screen.getByRole("button", { name: /pair/i })).toBeTruthy();
  });

  test("a named character is offered pairing by name", () => {
    render(AutofillView, props({ charOpen: true, charName: "Vex Aldenne" }));
    expect(screen.getByText(/remembered text lives/i).textContent).toContain("Vex Aldenne");
  });

  test("with no character open at all, the hint says to open one", () => {
    render(AutofillView, props({ charOpen: false }));
    expect(screen.getByText(/open a character/i)).toBeTruthy();
  });
});
