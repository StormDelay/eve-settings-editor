<script lang="ts">
  import type { Hud, HudEntry, HudKind, NeocomBar } from "$lib/api";
  import { EFFECT_COUNT_MAX, targetAnchor, targetFractionFromPoint, type FurnitureRect } from "$lib/layout";
  import NeocomButtons from "$lib/NeocomButtons.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";

  let {
    hud, readOnly, accountReadOnly = false, onSet, sharedNames = [], selectedKind = null, onSelectKind,
    targets, onTargets, effects, onEffects, referenceW = 0, referenceH = 0,
    neocom = null, neocomBusy = false, onNeocomReorder, onNeocomRemove, onNeocomAdd, onNeocomReset,
    hudError = null, neocomError = null,
  }: {
    hud: Hud;
    readOnly: boolean;
    /** Refused edits, owned by LayoutView (which runs the commands) and rendered
     *  here (which owns the controls). `hudError` carries the field's `name` so
     *  it lands under the row that failed rather than at the top of a panel with
     *  twenty rows in it. */
    hudError?: { name: string; text: string; detail: string } | null;
    neocomError?: { text: string; detail: string } | null;
    /** The ACCOUNT document's read-only flag, which only the account-scoped
     * rows care about. False when no account file is open — those rows are
     * already `unavailable` then. */
    accountReadOnly?: boolean;
    onSet: (name: string, text: string) => void;
    /** Other characters on this account, for the account-row legend. */
    sharedNames?: string[];
    /** The furniture selected on the canvas — its group is highlighted here. */
    selectedKind?: FurnitureRect["kind"] | null;
    /** Selecting a group here highlights the matching rectangle on the canvas,
     * the mirror of clicking that rectangle. */
    onSelectKind: (kind: FurnitureRect["kind"]) => void;
    /** How many locked targets the canvas draws the target list at, and the
     * setter for it. A VIEW setting, not a field: no settings file records how
     * many things a pilot locks, so the canvas has to be told. Badged `view`
     * in the row below so it cannot be mistaken for something that writes. */
    targets: number;
    onTargets: (n: number) => void;
    /** How many ship-effect icons the canvas draws under the HUD. A VIEW
     * setting for the same reason as `targets`, and badged the same way. */
    effects: number;
    onEffects: (n: number) => void;
    /** The open file's reference screen size. Only the target anchor uses it:
     * that pair is stored as a fraction, and the rows show the pixel it lands
     * on at this size instead. 0 means unknown — the rows then show the raw
     * fraction rather than a pixel computed from nothing. */
    referenceW?: number;
    referenceH?: number;
    /** The neocom bar, when a character file is open. Rendered inside the
     * Neocom group so the bar the user clicks on the canvas and the buttons
     * they edit are the same object. */
    neocom?: NeocomBar | null;
    /** True while a neocom command is in flight — disables the whole list so
     * a second click can't reuse a now-stale index before the re-projection
     * lands. */
    neocomBusy?: boolean;
    onNeocomReorder: (order: number[]) => void;
    onNeocomRemove: (index: number) => void;
    onNeocomAdd: (id: string, btnType: number, iconPath: string) => void;
    onNeocomReset: () => void;
  } = $props();

  // Display order and labels. `kind` ties a group to the canvas element it
  // edits, so selecting furniture highlights the fields that move it.
  // Account-scoped rows are badged because they change every character on the
  // account — the legend below names them.
  const GROUPS: { title: string; kind: FurnitureRect["kind"]; rows: { name: string; label: string }[] }[] = [
    { title: "Ship HUD", kind: "shipui", rows: [
      { name: "ship_offset", label: "Offset from centre" },
      { name: "ship_top", label: "Align to top" },
    ] },
    { title: "Fighter panel", kind: "fighter", rows: [
      { name: "fighter_x", label: "x" },
      { name: "fighter_y", label: "y" },
      { name: "fighter_detached", label: "Detached" },
      { name: "fighter_shown", label: "Shown" },
    ] },
    { title: "Neocom", kind: "neocom", rows: [{ name: "neocom_width", label: "Width" }] },
    { title: "Notification badge", kind: "badge", rows: [
      { name: "badge_x", label: "x" },
      { name: "badge_y", label: "y" },
    ] },
    // These two read and write like every other pixel row here, but the file
    // stores them as a FRACTION — see `shown`/`numberEdit` below, which convert
    // both ways. Dragging on the canvas is the ergonomic path; these are for
    // typing an exact position.
    { title: "Target list", kind: "target", rows: [
      { name: "target_x", label: "x" },
      { name: "target_y", label: "y" },
      { name: "target_horizontal", label: "Horizontal" },
    ] },
  ];

  // Only claim rows are shared when at least one of them is actually writable —
  // with no account file open they all read `unavailable`, and the claim would
  // be describing something the user cannot see or change.
  const anyAccountRow = $derived(
    hud.entries.some((e) => e.scope === "account" && e.set.how !== "unavailable"),
  );

  const find = (name: string): HudEntry | undefined => hud.entries.find((e) => e.name === name);
  const raw = (name: string) => find(name)?.value ?? find(name)?.default ?? "";

  // The target anchor is stored as a FRACTION of the screen — and not even a
  // fraction of the whole width, since x spans only what is right of the
  // neocom. Nobody wants to type 0.5442122186495176 to nudge it. These two rows
  // show the pixel that fraction lands on at the file's own reference size, and
  // convert back on write through the same pair the canvas drag uses, so the
  // client's own denominator survives the edit. Everything else here is already
  // a pixel and passes straight through.
  const AXIS: Record<string, "x" | "y"> = { target_x: "x", target_y: "y" };
  const asPixels = $derived(referenceW > 0 && referenceH > 0);
  const frac = (name: string) => {
    const n = parseFloat(raw(name));
    return Number.isFinite(n) ? n : 0;
  };
  const anchorPx = () => targetAnchor(frac("target_x"), frac("target_y"), referenceW, referenceH);
  const shown = (name: string) =>
    AXIS[name] && asPixels ? String(Math.round(anchorPx()[AXIS[name]])) : raw(name);
  // `readOnly` is the CHARACTER document's flag. An account-scoped row writes
  // the account file, so a read-only account left those four rows clickable and
  // the backend's refusal arrived as a dialog — stricter to say so up front.
  const disabled = (e: HudEntry) =>
    readOnly || e.set.how === "unavailable" || (e.scope === "account" && accountReadOnly);
  const title = (e: HudEntry) =>
    // The pixel note replaces only the account-wide line: "not present in this
    // file" and "EVE's default" still have to win, or the two anchor rows would
    // claim to be editable pixels while reading nothing.
    AXIS[e.name] && asPixels && e.set.how !== "unavailable" && e.value !== null
      ? `Screen pixels at ${referenceW}x${referenceH}. Stored as a fraction — account-wide.`
      : e.set.how === "unavailable"
      ? "Not present in this file"
      : e.value === null
        ? `EVE's default (${e.default}) — editing stores a value`
        : e.scope === "account"
          ? "Account-wide: changes every character on this account"
          : "";

  // The two `view` rows' shared handler. They write no settings field, so they
  // skip numberEdit entirely — but they need its blank guard: `Number("")` is
  // 0, and for the effect count 0 is a REAL count, so a cleared box would
  // silently wipe the row instead of leaving it alone. (The target count was
  // getting away with it only because its floor of 1 clamped the stray 0 back.)
  //
  // Resyncs on every commit for the same reason numberEdit does: a refused or
  // clamped entry must not leave typed text sitting beside a value that is not
  // it. `current` is read at event time, so a committed edit is overwritten by
  // the parent's re-render a moment later.
  const viewEdit = (current: () => number, set: (n: number) => void) => (ev: Event) => {
    const el = ev.target as HTMLInputElement;
    const text = el.value;
    if (text.trim() !== "" && Number.isFinite(Number(text))) set(Number(text));
    el.value = String(current());
  };

  // Int fields round before writing: <input type="number"> doesn't enforce
  // integrality (typing "1.5" is not blocked), and the backend's Int parser
  // rejects a fractional string outright.
  //
  // The field is put back in step with the model on every commit, whatever
  // happens next. Svelte only patches `value` when the EXPRESSION changes, so an
  // edit that does not move the model — one this refuses, one the backend
  // refuses, or an int rounding back to what it already was (326.4 -> 326) —
  // left the typed text sitting on screen beside a value that is not it. If the
  // write does land, the parent's re-render overwrites this a moment later.
  const numberEdit = (name: string, kind: HudKind) => (ev: Event) => {
    const el = ev.target as HTMLInputElement;
    const text = el.value;
    const resync = () => { el.value = shown(name); };
    if (text.trim() === "" || !Number.isFinite(Number(text))) {
      resync();
      return;
    }
    const axis = AXIS[name];
    if (axis && asPixels) {
      // Feed the OTHER axis its current pixel so this edit moves one axis only.
      const a = anchorPx();
      const next = targetFractionFromPoint(
        frac("target_x"),
        axis === "x" ? Number(text) : a.x,
        axis === "y" ? Number(text) : a.y,
        referenceW,
        referenceH,
      );
      onSet(name, String(next[axis]));
    } else {
      onSet(name, kind === "float" ? text : String(Math.round(Number(text))));
    }
    resync();
  };
</script>

<div class="hud-panel">
  {#if anyAccountRow}
    <!-- InlineMessage directly rather than ScopeBanner: this legend's text is
         not a string — it opens with a live `account` chip — and ScopeBanner
         takes a `label`. It is the same treatment either way, since ScopeBanner
         IS an info InlineMessage with a fixed shape. The class stays because
         HudPanel.spec finds the legend by it. -->
    <InlineMessage class="account-legend">
      <Chip tone="neutral" size="sm">account</Chip> rows are stored once for the whole account{sharedNames.length
        ? ` — editing one also changes ${sharedNames.join(", ")}`
        : " — every character on it"}.
    </InlineMessage>
  {/if}
  {#each GROUPS as g (g.title)}
    <div class="group" class:selected={selectedKind === g.kind}>
      <h4>
        <Button variant="ghost" class="group-title" onclick={() => onSelectKind(g.kind)}>{g.title}</Button>
      </h4>
      {#each g.rows as row (row.name)}
        {@const e = find(row.name)}
        {#if e}
          <!-- The row stays a wrapping <label> around a bare Field: the label
               names the control, and HudPanel.spec walks `.row input`. Passing
               Field a `label` here would nest one label inside another. -->
          <label class="row" title={title(e)}>
            {#if e.kind === "bool"}
              <Field
                kind="checkbox"
                value={shown(row.name) === "true"}
                disabled={disabled(e)}
                disabledReason="Not present in this file"
                onchange={(ev) => onSet(row.name, (ev.target as HTMLInputElement).checked ? "true" : "false")} />
              <span class="label">{row.label}</span>
            {:else}
              <span class="label">{row.label}</span>
              <Field
                kind="number"
                width="5.5rem"
                step={e.kind === "float" && !AXIS[row.name] ? undefined : 1}
                value={shown(row.name)}
                disabled={disabled(e)}
                disabledReason="Not present in this file"
                onchange={numberEdit(row.name, e.kind)} />
            {/if}
            {#if e.scope === "account"}<Chip tone="neutral" size="sm">account</Chip>{/if}
            {#if e.value === null && e.set.how !== "unavailable"}
              <Chip tone="neutral" size="sm">default</Chip>
            {/if}
          </label>
          {#if hudError?.name === row.name}
            <InlineMessage variant="error" detail={hudError.detail}>{hudError.text}</InlineMessage>
          {/if}
        {/if}
      {/each}
      {#if g.kind === "shipui"}
        <!-- Same `view` badge and same reasoning as "Targets drawn" below: no
             settings file records what is applied to a ship, so the canvas has
             to be told, and telling it writes nothing. -->
        <label class="row view" title="How many effect icons the canvas draws under the ship HUD. A view setting — it writes nothing.">
          <span class="label">Effects drawn</span>
          <Field
            kind="number"
            width="5.5rem"
            min={0}
            max={EFFECT_COUNT_MAX}
            step={1}
            value={effects}
            onchange={viewEdit(() => effects, onEffects)} />
          <Chip tone="neutral" size="sm">view</Chip>
        </label>
      {/if}
      {#if g.kind === "target"}
        <!-- `view`, not a field: it stays enabled on a read-only document,
             because changing it writes nothing. HudPanel.spec pins that. -->
        <label class="row view" title="How many locked targets the canvas draws. A view setting — it writes nothing.">
          <span class="label">Targets drawn</span>
          <Field
            kind="number"
            width="5.5rem"
            min={1}
            max={10}
            step={1}
            value={targets}
            onchange={viewEdit(() => targets, onTargets)} />
          <Chip tone="neutral" size="sm">view</Chip>
        </label>
      {/if}
      {#if g.kind === "neocom" && neocom}
        <NeocomButtons
          bar={neocom}
          {readOnly}
          busy={neocomBusy}
          onReorder={onNeocomReorder}
          onRemove={onNeocomRemove}
          onAdd={onNeocomAdd}
          onReset={onNeocomReset}
          error={neocomError} />
      {/if}
    </div>
  {/each}
</div>

<style>
  .hud-panel {
    border-bottom: 1px solid var(--border);
    padding: var(--s1) var(--s2);
    font-size: var(--t-caption);
  }
  .hud-panel :global(.account-legend) {
    margin: 0 0 var(--s2);
    padding: var(--s1) var(--s2);
  }
  .group {
    margin-bottom: var(--s1);
    /* Transparent by default so selecting a group doesn't shift the layout. */
    border-left: 2px solid transparent;
    padding-left: var(--s1);
  }
  /* The selected-group treatment shares --warn with the canvas's selected
     rectangle (LayoutView's .win.selected / .furniture.selected). The panel and
     the rectangle it describes must agree; the token is what makes them.
     This used to be a hardcoded pair of ambers in two files with a comment
     asking two humans to remember — the weakest possible way to couple two
     things. The no-hardcoded-hex guard now enforces it. */
  .group.selected {
    border-left-color: var(--warn);
    background: var(--warn-dim);
  }
  h4 {
    margin: var(--s1) 0;
  }
  /* A button so it's keyboard-reachable, styled as the heading it replaces
     (same pattern as WindowPanel's window-name button). */
  .hud-panel :global(.group-title) {
    padding: 0;
    color: var(--text-secondary);
    font-size: var(--t-caption);
    font-weight: 600;
    text-transform: uppercase;
  }
  .group.selected :global(.group-title) {
    color: var(--warn);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--s1);
    padding: 0;
  }
  .label {
    color: var(--text);
    min-width: 8.5rem;
  }
</style>
