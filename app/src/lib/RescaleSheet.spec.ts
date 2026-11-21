// Component test (vitest + jsdom). The maths lives in Rust and is tested there;
// what is only observable here is the prefill, the live "EVE lays this out on…"
// line, and what Apply sends.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import RescaleSheet from "$lib/RescaleSheet.svelte";
import { calls } from "$lib/test/setup";

function mount(referenceW = 2560, referenceH = 1440) {
  const onApplied = vi.fn();
  render(RescaleSheet, { referenceW, referenceH, onApplied, onClose: vi.fn() } as never);
  return { onApplied };
}

const field = (name: RegExp) => screen.getByLabelText(name) as HTMLInputElement | HTMLSelectElement;

describe("RescaleSheet", () => {
  test("prefills from this machine", async () => {
    calls.stub("client_display", { resolution: [1920, 1080], ui_scale: 1.25 });
    mount();
    await waitFor(() => expect(field(/width/i).value).toBe("1920"));
    expect(field(/height/i).value).toBe("1080");
    expect(field(/ui scale/i).value).toBe("1.25");
    expect(screen.getByText(/EVE lays this out on a 1536×864 desktop/)).toBeTruthy();
    expect(screen.getByText(/Saved for a 2560×1440 desktop/)).toBeTruthy();
  });

  test("falls back to the saved size", async () => {
    calls.stub("client_display", { resolution: null, ui_scale: 1 });
    mount();
    await waitFor(() => expect(field(/width/i).value).toBe("2560"));
    expect(field(/height/i).value).toBe("1440");
    expect(screen.getByText(/EVE lays this out on a 2560×1440 desktop/)).toBeTruthy();
  });

  test("apply sends the resolution and scale", async () => {
    calls.stub("client_display", { resolution: [1920, 1080], ui_scale: 1 });
    calls.stub("rescale_layout", { desktop_w: 1241, desktop_h: 698, windows_changed: 3, held: [] });
    const { onApplied } = mount();
    await waitFor(() => expect(field(/width/i).value).toBe("1920"));
    await fireEvent.input(field(/width/i), { target: { value: "1366" } });
    await fireEvent.input(field(/height/i), { target: { value: "768" } });
    await fireEvent.change(field(/ui scale/i), { target: { value: "1.1" } });
    expect(screen.getByText(/EVE lays this out on a 1241×698 desktop/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Rescale" }));
    await waitFor(() => expect(calls.only("rescale_layout").args).toEqual({ width: 1366, height: 768, uiScale: 1.1 }));
    await waitFor(() => expect(onApplied).toHaveBeenCalledOnce());
  });

  test("lists held windows", async () => {
    calls.stub("client_display", { resolution: [1920, 1080], ui_scale: 1 });
    calls.stub("rescale_layout", { desktop_w: 1920, desktop_h: 1080, windows_changed: 4, held: ["market"] });
    mount();
    await waitFor(() => expect(field(/width/i).value).toBe("1920"));
    await fireEvent.click(screen.getByRole("button", { name: "Rescale" }));
    await waitFor(() => expect(screen.getByText(/minimum or fixed size: market/)).toBeTruthy());
  });
});
