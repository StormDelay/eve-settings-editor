import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import NeocomButtons from "./NeocomButtons.svelte";
import type { NeocomBar, NeocomButton } from "./api";
import { confirm } from "@tauri-apps/plugin-dialog";

// Reset is gated on the Tauri dialog's confirm, not the bare browser one (see
// NeocomButtons.svelte's resetBar) — mock it so the confirmed/cancelled
// branches are both reachable from a test.
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: vi.fn() }));

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
    // Every one of them, not a sample: a control left live on a read-only file
    // reaches the backend and comes back as a dialog instead of being visibly
    // unavailable.
    render(NeocomButtons, props(bar([btn(0, "chat"), btn(1, "mail")], [btn(0, "wallet")]), true));
    expect((screen.getByLabelText("Move mail up") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Move chat down") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Remove chat") as HTMLButtonElement).disabled).toBe(true);
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

  test("confirming the reset dialog calls onReset", async () => {
    vi.mocked(confirm).mockResolvedValueOnce(true);
    const p = props(bar([btn(0, "chat")], [btn(0, "wallet")]));
    render(NeocomButtons, p);
    screen.getByText("Reset to original").click();
    await waitFor(() => expect(p.onReset).toHaveBeenCalled());
  });

  test("cancelling the reset dialog does not call onReset", async () => {
    vi.mocked(confirm).mockResolvedValueOnce(false);
    const p = props(bar([btn(0, "chat")], [btn(0, "wallet")]));
    render(NeocomButtons, p);
    screen.getByText("Reset to original").click();
    await waitFor(() => expect(vi.mocked(confirm)).toHaveBeenCalled());
    expect(p.onReset).not.toHaveBeenCalled();
  });
});
