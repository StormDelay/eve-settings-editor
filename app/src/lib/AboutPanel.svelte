<script lang="ts">
  // The version comes from the runtime, which reads it out of tauri.conf.json —
  // NOT from a constant here. A hardcoded string is a second number to bump at
  // release, and the one that gets forgotten.
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";

  let { onClose }: { onClose: () => void } = $props();

  const REPO = "https://github.com/StormDelay/eve-settings-editor";

  // Resolved once at mount, like Sidebar's own `refresh()`. Shown as "…" until
  // it lands rather than blank, so an empty box never reads as "no version".
  let version = $state("…");
  getVersion()
    .then((v) => (version = v))
    .catch(() => (version = "unknown"));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" role="none" data-testid="about-backdrop" onclick={onClose}>
  <!-- `tabindex="-1"` because a dialog must be focusable to be reachable by
       anything but the mouse; -1 keeps it out of the tab order itself. -->
  <div class="modal" role="dialog" aria-label="About" tabindex="-1"
       onclick={(e) => e.stopPropagation()}>
    <h2>EVE Settings Editor</h2>
    <p class="version">Version {version}</p>
    <p>
      <button class="linkbtn" onclick={() => void openUrl(REPO).catch(() => {})}>
        Source and issues on GitHub
      </button>
    </p>
    <p class="meta">
      MIT licensed. An unofficial tool — EVE Online is a trademark of CCP hf.
    </p>
    <div class="form-actions">
      <span class="spacer"></span>
      <button onclick={onClose}>Close</button>
    </div>
  </div>
</div>

<style>
  /* `.overlay`, `.modal`, `.form-actions` and `.spacer` are global (app.css). */
  h2 { margin: 0 0 0.2rem; font-size: 1em; font-weight: 600; }
  .version { margin: 0 0 0.8rem; color: var(--fg-dim); }
  .meta { color: var(--fg-dim); font-size: 0.85em; margin: 0.8rem 0 0; }
  /* Matches BatchView's own link-shaped button. */
  .linkbtn { background: none; border: none; color: var(--accent); cursor: pointer; font: inherit; padding: 0; }
  .linkbtn:hover { text-decoration: underline; }
</style>
