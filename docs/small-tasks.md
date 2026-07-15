# Small tasks ledger

A holding pen for small, non-urgent improvements the developer wants done
*eventually*. These are **not** milestone-blocking on their own — they are
nice-to-haves that get **revisited at the end of every milestone**, before
release, and each open item is weighed for inclusion in that release.

Workflow:
- Add items here as they come up, newest anywhere in **Open**.
- At each milestone's end (before release), review every **Open** item with the
  developer for possible inclusion.
- When an item ships, move it to **Shipped** with the milestone that included it.

## Open

- [ ] **Resize layout windows from any corner.** In the layout canvas, a
  selected window can only be resized from the bottom-right handle today. Add
  resize handles on all four corners (edges optional) once a window is selected,
  so it can be resized from any corner. _Added 2026-07-15._

- [ ] **Persist the "Hide non-standard files" toggle.** The sidebar toggle
  defaults to on but resets every launch (the app has no settings store). Give
  it a small persistent store so the choice is remembered between sessions.
  _Added 2026-07-15 (M3a)._

- [ ] **Collapsible side panels.** Make the side panels (sidebar file list and
  backups panel) retractable/collapsible so the center pane can grow — useful
  when editing window placements on the layout canvas, which wants as much
  horizontal room as possible. _Added 2026-07-15._

## Shipped

### M3

- [x] **Negative-cache invalid character IDs.** ESI 404s any ID it can't
  resolve; those IDs are never cached, so every launch re-bisects them (extra
  ESI requests, counting against the error limit). Cache a tombstone for
  known-invalid IDs so they're skipped until a manual refresh. _Added
  2026-07-15 (M3a)._

- [x] **Name dialog-opened char files.** The open-file header only shows a
  character name for files discovered by the standard scan; a `core_char_<id>.dat`
  opened via the "Open file…" dialog shows a bare filename. Resolve its name on
  open too. _Added 2026-07-15 (M3a)._

- [x] **Extend name display to more surfaces.** Character names currently show
  in the sidebar and the open-file header only. Add them to the backups panel
  and the native OS window title. _Added 2026-07-15 (M3a)._
