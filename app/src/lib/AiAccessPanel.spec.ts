// Component test: vitest + jsdom.
import { describe, expect, test, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import AiAccessPanel from "$lib/AiAccessPanel.svelte";
import { api, type McpSetup } from "$lib/api";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import "$lib/test/setup";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn(() => Promise.resolve()) }));

const base: McpSetup = {
  command: "C:\\Program Files\\EVE Settings Editor\\eve-settings-editor.exe",
  args: ["--mcp"],
  snippet: '{\n  "mcpServers": {\n    "eve-settings-editor": {}\n  }\n}',
  claude_desktop: null,
};

describe("AiAccessPanel", () => {
  test("shows the snippet and copies it", async () => {
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(base);
    render(AiAccessPanel, { onClose: () => {} });

    expect(await screen.findByText(/"eve-settings-editor"/)).toBeTruthy();
    await fireEvent.click(screen.getByText("Copy"));
    expect(vi.mocked(writeText)).toHaveBeenCalledWith(base.snippet);
  });

  test("without a Claude Desktop config dir there is no Register row", async () => {
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(base);
    render(AiAccessPanel, { onClose: () => {} });
    await screen.findByText(/"eve-settings-editor"/);
    expect(screen.queryByText("Register")).toBeNull();
    expect(screen.getByText(/Claude Desktop is not installed/)).toBeTruthy();
  });

  test("Register calls the command and re-renders from its result", async () => {
    const off = { ...base, claude_desktop: { config_path: "C:\\x\\claude_desktop_config.json", registered: false } };
    const on = { ...off, claude_desktop: { ...off.claude_desktop!, registered: true } };
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(off);
    const set = vi.spyOn(api, "mcpSetClaudeDesktop").mockResolvedValue(on);
    render(AiAccessPanel, { onClose: () => {} });

    await fireEvent.click(await screen.findByText("Register"));
    expect(set).toHaveBeenCalledWith(true);
    expect(await screen.findByText("Unregister")).toBeTruthy();
    expect(screen.getByText(/Registered/)).toBeTruthy();
  });

  test("a failed register shows the error and keeps the row", async () => {
    const off = { ...base, claude_desktop: { config_path: "C:\\x\\claude_desktop_config.json", registered: false } };
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(off);
    vi.spyOn(api, "mcpSetClaudeDesktop").mockRejectedValue({ code: "parse", message: "not JSON" });
    render(AiAccessPanel, { onClose: () => {} });

    await fireEvent.click(await screen.findByText("Register"));
    expect(await screen.findByText(/not JSON/)).toBeTruthy();
    expect(screen.getByText("Register")).toBeTruthy();
  });

  test("Close dismisses it", async () => {
    vi.spyOn(api, "mcpSetupInfo").mockResolvedValue(base);
    const onClose = vi.fn();
    render(AiAccessPanel, { onClose });
    await screen.findByText(/"eve-settings-editor"/);
    await fireEvent.click(screen.getByText("Close"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
