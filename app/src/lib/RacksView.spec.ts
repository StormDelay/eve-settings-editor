// Component test: run with `npm test` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen, waitFor, within } from "@testing-library/svelte";
import RacksView from "$lib/RacksView.svelte";
import { calls } from "$lib/test/setup";
import type { SlotEntry } from "$lib/api";

const DEFAULT = [27, 28, 29, 30, 31, 32, 33, 34, 19, 20, 21, 22, 23, 24, 25, 26, 11, 12, 13, 14, 15, 16, 17, 18];
const SHIPS: SlotEntry[] = [
  { ship_id: 90000001, order: DEFAULT },
  { ship_id: 90000002, order: null },
];

function mount(ships: SlotEntry[] = SHIPS, dirty: () => void = () => {}) {
  for (const cmd of ["slot_orders", "slot_order_add", "slot_order_remove", "slot_order_swap", "slot_order_copy"]) {
    calls.stub(cmd, ships);
  }
  return render(RacksView, { userOpen: true, userId: 1, onUserDirty: dirty });
}

const rack = (name: string) => screen.getByRole("group", { name });

describe("the racks view", () => {
  test("lists every ship and draws the selected one's three racks by label", async () => {
    mount();
    await fireEvent.click(await screen.findByText("90000001"));
    expect(within(rack("Top rack")).getAllByRole("button").map((b) => b.textContent)).toEqual(
      ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8"]);
    expect(within(rack("Middle rack")).getAllByRole("button")[0].textContent).toBe("M1");
    expect(within(rack("Bottom rack")).getAllByRole("button")[7].textContent).toBe("L8");
  });

  test("clicking two slots swaps them by flag and marks the account dirty", async () => {
    let dirty = 0;
    mount(SHIPS, () => dirty++);
    await fireEvent.click(await screen.findByText("90000001"));
    await fireEvent.click(within(rack("Top rack")).getByText("H1"));
    await fireEvent.click(within(rack("Middle rack")).getByText("M1"));
    await waitFor(() => expect(calls.of("slot_order_swap")).toHaveLength(1));
    expect(calls.of("slot_order_swap")[0].args).toEqual({ shipId: 90000001, a: 27, b: 19 });
    expect(dirty).toBe(1);
  });

  test("dropping one slot on another swaps them", async () => {
    mount();
    await fireEvent.click(await screen.findByText("90000001"));
    const data = new Map<string, string>();
    const dataTransfer = { setData: (k: string, v: string) => data.set(k, v), getData: (k: string) => data.get(k) ?? "" };
    await fireEvent.dragStart(within(rack("Top rack")).getByText("H2"), { dataTransfer });
    await fireEvent.drop(within(rack("Bottom rack")).getByText("L1"), { dataTransfer });
    await waitFor(() => expect(calls.of("slot_order_swap")[0]?.args).toEqual({ shipId: 90000001, a: 28, b: 11 }));
  });

  test("an unreadable ship says so and can only be removed", async () => {
    mount();
    await fireEvent.click(await screen.findByText(/90000002/));
    expect(screen.getByText(/shape the editor doesn't read/)).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Top rack" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Remove ship" }));
    await waitFor(() => expect(calls.of("slot_order_remove")[0]?.args).toEqual({ shipId: 90000002 }));
  });

  test("add starts from the selected ship and refuses an id already listed", async () => {
    mount();
    await fireEvent.click(await screen.findByText("90000001"));
    const id = screen.getByLabelText("Ship id") as HTMLInputElement;
    await fireEvent.input(id, { target: { value: "90000001" } });
    expect((screen.getByRole("button", { name: "Add ship" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.input(id, { target: { value: "90000005" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add ship" }));
    await waitFor(() => expect(calls.of("slot_order_add")[0]?.args).toEqual({ shipId: 90000005, from: 90000001 }));
  });

  test("copy sends the ticked ships", async () => {
    mount([...SHIPS, { ship_id: 90000003, order: DEFAULT }]);
    await fireEvent.click(await screen.findByText("90000001"));
    await fireEvent.click(screen.getByLabelText("Copy to 90000003"));
    await fireEvent.click(screen.getByRole("button", { name: "Copy layout" }));
    await waitFor(() => expect(calls.of("slot_order_copy")[0]?.args).toEqual({ from: 90000001, to: [90000003] }));
  });

  test("copy skips a ticked ship the filter has hidden", async () => {
    mount([...SHIPS, { ship_id: 90000003, order: DEFAULT }, { ship_id: 90000004, order: DEFAULT }]);
    await fireEvent.click(await screen.findByText("90000001"));
    await fireEvent.click(screen.getByLabelText("Copy to 90000003"));
    await fireEvent.input(screen.getByRole("searchbox"), { target: { value: "90000004" } });
    await fireEvent.click(await screen.findByLabelText("Copy to 90000004"));
    expect(screen.queryByLabelText("Copy to 90000003")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Copy layout" }));
    await waitFor(() => expect(calls.of("slot_order_copy")[0]?.args).toEqual({ from: 90000001, to: [90000004] }));
  });

  test("a refused write shows the backend's sentence", async () => {
    mount();
    calls.stub("slot_order_swap", () => Promise.reject({ code: "missing", message: "Ship 90000001 has no layout here." }));
    await fireEvent.click(await screen.findByText("90000001"));
    await fireEvent.click(within(rack("Top rack")).getByText("H1"));
    await fireEvent.click(within(rack("Top rack")).getByText("H2"));
    expect(await screen.findByText(/has no layout here/)).toBeTruthy();
  });
});
