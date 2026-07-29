// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// PresetGroup's "New from open character…" form is the only place a preset
// gets created, and the checkboxes it offers must reflect which file(s) the
// backend will actually write. This spec covers the one rule with a real
// consequence if wrong: an aspect that needs the account file open must be
// disabled while it isn't, same as BatchView disables an unpaired target for
// an account-scoped aspect.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import PresetGroup from "$lib/PresetGroup.svelte";
import { calls } from "$lib/test/setup";

async function renderPresets(props: { charOpen?: boolean; userOpen?: boolean } = {}) {
  calls.stub("settings_preset_list", []);
  render(PresetGroup, {
    onOpenPreset: () => {},
    charOpen: true,
    userOpen: true,
    openPresetName: null,
    ...props,
  });
  await fireEvent.click(screen.getByRole("button", { name: /new from open character/i }));
  return {
    aspect: (label: string) =>
      [...screen.getAllByRole("checkbox")].find((c) => c.closest("label")?.textContent?.includes(label))! as HTMLInputElement,
  };
}

describe("which aspects the create form offers", () => {
  test("creating a layout preset needs the account file open", async () => {
    const { aspect } = await renderPresets({ userOpen: false });
    await waitFor(() => expect(aspect("Window layout").disabled).toBe(true));
  });

  test("layout is available once the account file is open", async () => {
    const { aspect } = await renderPresets({ userOpen: true });
    await waitFor(() => expect(aspect("Window layout").disabled).toBe(false));
  });
});
