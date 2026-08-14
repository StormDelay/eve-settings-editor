// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The rules worth pinning are the ones a type check cannot see: a blank input
// writes nothing, the stack button only appears for a real stack and names the
// right count, and the history area reports the overflow case rather than
// hiding it.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ChatSplit from "$lib/ChatSplit.svelte";
import type { ChatPanel, Stack } from "$lib/api";

const panel = (userlist: number | null, input: number | null): ChatPanel => ({
  window_id: "chatchannel_local",
  userlist_width: userlist,
  input_height: input,
});

const stack = (members: string[]): Stack => ({
  container_id: "ChatWindowStack",
  container_label: "Chat stack",
  anchor_id: members[0],
  members,
});

type Props = {
  windowId: string;
  geom: { w: number; h: number } | null;
  panel: ChatPanel | undefined;
  stack: Stack | null;
  readOnly: boolean;
  sharedNames: string[];
  onSet: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void;
};

function setup(over: Partial<Props> = {}) {
  const onSet = vi.fn();
  render(ChatSplit, {
    windowId: "chatchannel_local",
    geom: { w: 256, h: 424 },
    panel: panel(104, 63),
    stack: null,
    readOnly: false,
    sharedNames: [],
    onSet,
    ...over,
  } satisfies Props);
  return { onSet };
}

describe("ChatSplit", () => {
  test("shows the history area the splits leave", () => {
    setup();
    expect(screen.getByText(/History area 152 × 361/)).toBeTruthy();
  });

  test("flags an oversized split instead of hiding it", () => {
    setup({ panel: panel(300, 500) });
    // Negative, not clamped: the account-wide value does not fit this window.
    expect(screen.getByText(/History area -44 × -76/)).toBeTruthy();
  });

  test("a blank input writes nothing", async () => {
    const { onSet } = setup();
    const input = screen.getByLabelText("Member list") as HTMLInputElement;
    await fireEvent.change(input, { target: { value: "" } });
    expect(onSet).not.toHaveBeenCalled();
    expect(input.value).toBe("104");
  });

  test("a number writes only its own field", async () => {
    const { onSet } = setup();
    await fireEvent.change(screen.getByLabelText("Member list"), { target: { value: "120" } });
    expect(onSet).toHaveBeenCalledWith(["chatchannel_local"], 120, null);
  });

  test("no stack button when the window is not stacked", () => {
    setup();
    expect(screen.queryByRole("button", { name: /Apply to every channel/ })).toBeNull();
  });

  test("the stack button names the chat channels only", async () => {
    const { onSet } = setup({ stack: stack(["chatchannel_local", "market", "chatchannel_corp"]) });
    const button = screen.getByRole("button", { name: "Apply to every channel in this stack (2)" });
    await fireEvent.click(button);
    expect(onSet).toHaveBeenCalledWith(["chatchannel_local", "chatchannel_corp"], 104, 63);
  });

  test("read-only disables the inputs", () => {
    setup({ readOnly: true });
    expect((screen.getByLabelText("Member list") as HTMLInputElement).disabled).toBe(true);
  });

  test("the stack button is disabled when there is nothing to copy", () => {
    setup({ panel: panel(null, null), stack: stack(["chatchannel_local", "chatchannel_corp"]) });
    const button = screen.getByRole("button", { name: /Apply to every channel/ }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });
});
