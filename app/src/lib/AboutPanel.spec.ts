// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import AboutPanel from "$lib/AboutPanel.svelte";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
// Imported for its afterEach cleanup: without it every render stays in the
// document and the next test's queries match two copies of everything.
import "$lib/test/setup";

// Both are Tauri boundaries jsdom cannot cross. `getVersion` reads
// tauri.conf.json through the runtime, which is the whole point — the version
// is never a constant in the frontend.
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(() => Promise.resolve()) }));

describe("AboutPanel", () => {
  test("shows the version the runtime reports", async () => {
    vi.mocked(getVersion).mockResolvedValue("0.29.0");
    render(AboutPanel, { onClose: () => {} });

    expect(await screen.findByText("Version 0.29.0")).toBeTruthy();
  });

  test("a version that cannot be read says so rather than showing nothing", async () => {
    // A blank where the number belongs reads as "no version", which is worse
    // than admitting the lookup failed.
    vi.mocked(getVersion).mockRejectedValue(new Error("no runtime"));
    render(AboutPanel, { onClose: () => {} });

    expect(await screen.findByText("Version unknown")).toBeTruthy();
  });

  test("the repo link opens in the system browser, not the webview", async () => {
    vi.mocked(getVersion).mockResolvedValue("0.29.0");
    render(AboutPanel, { onClose: () => {} });

    await fireEvent.click(screen.getByText("Source and issues on GitHub"));

    expect(vi.mocked(openUrl)).toHaveBeenCalledWith(
      "https://github.com/StormDelay/eve-settings-editor",
    );
  });

  test("Close and the backdrop both dismiss it", async () => {
    vi.mocked(getVersion).mockResolvedValue("0.29.0");
    const onClose = vi.fn();
    render(AboutPanel, { onClose });

    await fireEvent.click(screen.getByText("Close"));
    expect(onClose).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByTestId("about-backdrop"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
