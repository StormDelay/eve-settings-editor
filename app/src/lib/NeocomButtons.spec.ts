import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import NeocomButtons from "./NeocomButtons.svelte";
import type { NeocomBar, NeocomButton } from "./api";
import { toasts } from "./ui/toasts.svelte";

// No dialog mock, and that is the assertion: the reset stopped being a
// confirmation. It is an in-memory edit that Discard reverses exactly, and the
// button's own tooltip states the consequence before the click.

const btn = (index: number, id: string, children = 0): NeocomButton =>
  ({ index, id, btn_type: 1, icon_path: `${id}.png`, children });

const bar = (buttons: NeocomButton[], original: NeocomButton[] = []): NeocomBar =>
  ({ buttons, original });

const props = (b: NeocomBar, readOnly = false) => ({
  bar: b, readOnly,
  onReorder: vi.fn(), onRemove: vi.fn(), onAdd: vi.fn(), onReset: vi.fn(),
});

describe("NeocomButtons", () => {
  test("lists the buttons in bar order, under friendly labels", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail"), btn(2, "wallet")])));
    const rows = screen.getAllByTitle(/\.png$/);
    expect(rows.map((e) => e.textContent)).toEqual(["Chat", "EVE Mail", "Wallet"]);
    // The raw id stays reachable, because it is what the file actually says.
    expect(rows.map((e) => e.getAttribute("title"))).toEqual(
      ["chat — chat.png", "mail — mail.png", "wallet — wallet.png"],
    );
  });

  test("a button nobody has curated shows its raw id", () => {
    // The safe failure direction: an unknown id is ugly but checkable against
    // the file, where a plausible-looking wrong label would not be.
    render(NeocomButtons, props(bar([btn(0, "someFutureBtn")])));
    expect(screen.getAllByTitle(/\.png$/).map((e) => e.textContent)).toEqual(["someFutureBtn"]);
  });

  test("the end rows cannot move past the ends", () => {
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail")])));
    expect((screen.getByLabelText("Move Chat up") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Move EVE Mail down") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Move Chat down") as HTMLButtonElement).disabled).toBe(false);
  });

  test("moving a button sends the whole permutation", async () => {
    const p = props(bar([btn(0, "chat"), btn(1, "mail"), btn(2, "wallet")]));
    render(NeocomButtons, p);
    screen.getByLabelText("Move Wallet up").click();
    expect(p.onReorder).toHaveBeenCalledWith([0, 2, 1]);
  });

  test("the add list excludes what is already on the bar", () => {
    render(NeocomButtons, props(bar([btn(0, "chat")], [btn(0, "chat"), btn(1, "mail")])));
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toContain("EVE Mail");
    expect(options).not.toContain("Chat");
  });

  test("reset is disabled when the character has no original", () => {
    render(NeocomButtons, props(bar([btn(0, "chat")], [])));
    expect((screen.getByText("Reset to original") as HTMLButtonElement).disabled).toBe(true);
  });

  test("read-only disables every control", () => {
    // Every one of them, not a sample: a control left live on a read-only file
    // reaches the backend and comes back as a dialog instead of being visibly
    // unavailable.
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail")], [btn(0, "wallet")]), true));
    expect((screen.getByLabelText("Move EVE Mail up") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Move Chat down") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Remove Chat") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Add a neocom button") as HTMLSelectElement).disabled).toBe(true);
    expect((screen.getByText("Add") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText("Reset to original") as HTMLButtonElement).disabled).toBe(true);
  });

  test("a failed add keeps the choice in the dropdown", async () => {
    // The bar prop does not change when the command fails, so the id stays
    // addable — and the pick must still be there to retry.
    const p = props(bar([btn(0, "chat")], [btn(0, "chat"), btn(1, "mail")]));
    render(NeocomButtons, p);
    const select = screen.getByLabelText("Add a neocom button") as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: "mail" } });
    await fireEvent.click(screen.getByText("Add")); // fireEvent, so the state flush lands
    expect(p.onAdd).toHaveBeenCalledWith("mail", 1, "mail.png");
    await waitFor(() => expect(select.value).toBe("mail"));
  });

  test("a successful add clears the dropdown", async () => {
    const p = props(bar([btn(0, "chat")], [btn(0, "chat"), btn(1, "mail")]));
    const { rerender } = render(NeocomButtons, p);
    const select = screen.getByLabelText("Add a neocom button") as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: "mail" } });
    await fireEvent.click(screen.getByText("Add"));
    // What a successful add looks like from here: mail is on the bar now, so it
    // is no longer addable.
    await rerender({ ...p, bar: bar([btn(0, "chat"), btn(1, "mail")], [btn(0, "chat"), btn(1, "mail")]) });
    await waitFor(() => expect(select.value).toBe(""));
  });

  test("reset happens on the click and reports itself in a toast", async () => {
    toasts.length = 0;
    const p = props(bar([btn(0, "chat")], [btn(0, "wallet")]));
    render(NeocomButtons, p);
    screen.getByText("Reset to original").click();
    await waitFor(() => expect(p.onReset).toHaveBeenCalled());
    expect(toasts.map((t) => t.message)).toContain(
      "Neocom reset to the client's original buttons.",
    );
  });

  test("a refused neocom edit renders at the control, not in a dialog", () => {
    const p = props(bar([btn(0, "chat")], [btn(0, "wallet")]));
    render(NeocomButtons, {
      ...p,
      error: { text: "The neocom wasn't changed — the file is read-only.", detail: "[io] …" },
    });
    expect(screen.getByRole("alert").textContent).toContain("The neocom wasn't changed");
  });
});
