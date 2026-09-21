<script lang="ts">
  // The "AI access" sheet: hands the user a working MCP config with zero
  // typing. Register/Unregister writes Claude Desktop's file; every other
  // client takes the snippet below it. Spec §6 of the MCP design.
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { api, errMessage, type McpSetup } from "./api";
  import Button from "./ui/Button.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import Sheet from "./ui/Sheet.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let info = $state<McpSetup | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let copied = $state(false);

  api
    .mcpSetupInfo()
    .then((i) => (info = i))
    .catch((e) => (error = errMessage(e)));

  async function setClaude(on: boolean) {
    busy = true;
    error = null;
    try {
      info = await api.mcpSetClaudeDesktop(on);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }

  async function copy() {
    if (!info) return;
    await writeText(info.snippet).catch(() => {});
    copied = true;
  }
</script>

<Sheet title="AI access" width="min(560px, 92vw)" onclose={onClose} data-testid="ai-access-backdrop">
  <p>
    Let an AI assistant edit your settings through this app — overview, probe formations, window
    layout, keybinds, Neocom, HUD, fleet and chat, plus copy settings and presets. Every change goes
    through the same backups and checks as editing here.
  </p>

  {#if info}
    {@const cd = info.claude_desktop}
    <h3>Claude Desktop</h3>
    {#if cd}
      <p class="row">
        <span>{cd.registered ? "Registered" : "Not registered"}</span>
        <Button disabled={busy} onclick={() => void setClaude(!cd.registered)}>
          {cd.registered ? "Unregister" : "Register"}
        </Button>
      </p>
      <p class="meta">Restart Claude Desktop to pick this up.</p>
    {:else}
      <p class="meta">Claude Desktop is not installed on this machine — use the snippet below in another client.</p>
    {/if}

    <h3>Any other MCP client</h3>
    <pre>{info.snippet}</pre>
    <p class="row">
      <Button variant="ghost" onclick={() => void copy()}>Copy</Button>
      {#if copied}<span class="meta">Copied</span>{/if}
    </p>
    <p class="meta">
      Paste it into the client's MCP config. VS Code uses a <code>servers</code> key and Codex a
      <code>[mcp_servers.…]</code> TOML table — same command and args.
    </p>
  {/if}

  {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}

  {#snippet footer()}
    <Button onclick={onClose}>Close</Button>
  {/snippet}
</Sheet>

<style>
  h3 {
    margin: var(--s3) 0 var(--s1);
    font-size: var(--t-ui);
    font-weight: 600;
  }
  pre {
    overflow-x: auto;
    padding: var(--s2);
    background: var(--surface-raised);
    border-radius: var(--r-sm);
    font-size: var(--t-caption);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--s3);
  }
  .meta {
    color: var(--text-muted);
    font-size: var(--t-caption);
    margin: var(--s1) 0 0;
  }
</style>
