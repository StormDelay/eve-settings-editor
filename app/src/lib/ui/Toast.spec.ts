// Component test: vitest + jsdom.
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/svelte";
import Toast from "./Toast.svelte";
import { dismiss, toast, toasts } from "./toasts.svelte";
import "$lib/test/setup";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  for (const t of [...toasts]) dismiss(t.id);
});

describe("Toast", () => {
  test("the region is polite, not assertive — a confirmation must not interrupt", () => {
    const { container } = render(Toast, {});
    const region = container.querySelector(".toasts")!;
    expect(region.getAttribute("aria-live")).toBe("polite");
  });

  test("a message appears and clears itself after its duration", async () => {
    render(Toast, {});

    await act(() => toast("Saved core_user_.dat"));
    expect(screen.getByText("Saved core_user_.dat")).toBeTruthy();

    await act(() => vi.advanceTimersByTime(3999));
    expect(screen.queryByText("Saved core_user_.dat")).toBeTruthy();

    await act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByText("Saved core_user_.dat")).toBeNull();
  });

  // An error you did not read is an error you cannot act on.
  test("an error stays until it is dismissed", async () => {
    render(Toast, {});

    await act(() => toast("Could not write the file", { variant: "error" }));
    await act(() => vi.advanceTimersByTime(60_000));

    expect(screen.getByText("Could not write the file")).toBeTruthy();

    await act(() => fireEvent.click(screen.getByRole("button", { name: "Dismiss" })));
    expect(screen.queryByText("Could not write the file")).toBeNull();
  });

  test("duration 0 persists whatever the variant", async () => {
    render(Toast, {});

    await act(() => toast("Still here", { duration: 0 }));
    await act(() => vi.advanceTimersByTime(60_000));

    expect(screen.getByText("Still here")).toBeTruthy();
  });

  test("an action runs and takes the toast with it", async () => {
    const run = vi.fn();
    render(Toast, {});

    await act(() => toast("Tab deleted", { action: { label: "Undo", run } }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Undo" })));

    expect(run).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Tab deleted")).toBeNull();
  });

  test("several stack rather than replacing each other", async () => {
    render(Toast, {});

    await act(() => toast("First"));
    await act(() => toast("Second"));

    expect(screen.getByText("First")).toBeTruthy();
    expect(screen.getByText("Second")).toBeTruthy();
  });

  test("dismissing one leaves the others and their timers alone", async () => {
    render(Toast, {});

    await act(() => toast("Keep me", { duration: 0 }));
    await act(() => toast("Drop me", { duration: 0 }));

    const first = toasts[0].id;
    await act(() => dismiss(first));

    expect(screen.queryByText("Keep me")).toBeNull();
    expect(screen.getByText("Drop me")).toBeTruthy();
  });
});
