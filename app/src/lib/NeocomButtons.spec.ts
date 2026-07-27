import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import NeocomButtons from "./NeocomButtons.svelte";
import type { NeocomBar, NeocomButton } from "./api";

const btn = (index: number, id: string, children = 0): NeocomButton =>
  ({ index, id, btn_type: 1, icon_path: `${id}.png`, children });

const bar = (buttons: NeocomButton[], original: NeocomButton[] = []): NeocomBar =>
  ({ buttons, original });

const props = (b: NeocomBar, readOnly = false) => ({
  bar: b, readOnly,
  onReorder: vi.fn(), onRemove: vi.fn(), onAdd: vi.fn(), onReset: vi.fn(),
});

describe("NeocomButtons", () => {
  test("lists the buttons in bar order", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail"), btn(2, "wallet")])));
    const ids = screen.getAllByTitle(/\.png$/).map((e) => e.textContent);
    expect(ids).toEqual(["chat", "mail", "wallet"]);
  });

  test("the end rows cannot move past the ends", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail")])));
    expect((screen.getByLabelText("Move chat up") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Move mail down") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Move chat down") as HTMLButtonElement).disabled).toBe(false);
  });

  test("moving a button sends the whole permutation", async () => {
    const p = props(bar([btn(0, "chat"), btn(1, "mail"), btn(2, "wallet")]));
    render(NeocomButtons, p);
    screen.getByLabelText("Move wallet up").click();
    expect(p.onReorder).toHaveBeenCalledWith([0, 2, 1]);
  });

  test("the add list excludes what is already on the bar", () => {
    render(NeocomButtons, props(bar([btn(0, "chat")], [btn(0, "chat"), btn(1, "mail")])));
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toContain("mail");
    expect(options).not.toContain("chat");
  });

  test("reset is disabled when the character has no original", () => {
    render(NeocomButtons, props(bar([btn(0, "chat")], [])));
    expect((screen.getByText("Reset to original") as HTMLButtonElement).disabled).toBe(true);
  });

  test("read-only disables every control", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail")], [btn(0, "wallet")]), true));
    expect((screen.getByLabelText("Move chat down") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Remove chat") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText("Reset to original") as HTMLButtonElement).disabled).toBe(true);
  });
});
