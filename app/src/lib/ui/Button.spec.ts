// Component test: vitest + jsdom.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import Button from "./Button.svelte";
import { text } from "./snippet";
import "$lib/test/setup";

const style = /<style>([\s\S]*)<\/style>/.exec(
  readFileSync(resolve(import.meta.dirname, "Button.svelte"), "utf8"),
)?.[1] ?? "";

describe("Button", () => {
  // The regression this component exists for. `.mini { opacity: 0 }` revealed
  // only through `.row:hover .mini` left four buttons permanently invisible and
  // permanently clickable, two of which destroy data. A ghost button recedes by
  // being transparent, never by being hidden.
  test("a ghost button is never hidden — no opacity, visibility or display trick", () => {
    expect(style).not.toMatch(/opacity:\s*0\s*[;}]/);
    expect(style).not.toMatch(/visibility:\s*hidden/);
    expect(style).not.toMatch(/display:\s*none/);
  });

  test("the only opacity it declares is the disabled token", () => {
    const values = [...style.matchAll(/opacity:\s*([^;]+);/g)].map((m) => m[1].trim());
    expect(values).toEqual(["var(--o-disabled)"]);
  });

  // Asserts the `disabled` attribute rather than a suppressed click: fireEvent
  // dispatches the event directly, so jsdom does not reproduce the browser's
  // refusal to fire click on a disabled control. The attribute is the thing the
  // browser acts on, so it is the thing worth pinning.
  test("disabled is a real attribute, and it says why", () => {
    render(Button, {
      disabled: true,
      disabledReason: "The file is read-only",
      onclick: vi.fn(),
      children: text("Save"),
    });

    const btn = screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(btn.getAttribute("title")).toBe("The file is read-only");
  });

  test("an enabled button still calls onclick", async () => {
    const onclick = vi.fn();
    render(Button, { onclick, children: text("Go") });

    await fireEvent.click(screen.getByRole("button", { name: "Go" }));

    expect(onclick).toHaveBeenCalledTimes(1);
  });

  test("pressed renders aria-pressed, and its absence leaves the attribute off", () => {
    const { unmount } = render(Button, { pressed: true, children: text("Bold") });
    expect(screen.getByRole("button", { name: "Bold" }).getAttribute("aria-pressed")).toBe("true");
    unmount();

    render(Button, { children: text("Plain") });
    expect(screen.getByRole("button", { name: "Plain" }).hasAttribute("aria-pressed")).toBe(false);
  });

  test("iconOnly takes its accessible name from title", () => {
    render(Button, { iconOnly: true, title: "Remove", children: text("×") });
    expect(screen.getByRole("button", { name: "Remove" })).toBeTruthy();
  });

  // A glyph is not a label, and no lint rule can see that. Failing at
  // construction is the only thing that reliably stops it shipping.
  test("iconOnly without a title throws in dev", () => {
    expect(() => render(Button, { iconOnly: true, children: text("×") })).toThrow(/iconOnly/);
  });
});
