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

- [ ] **Wrap controls so the Save button stays visible when the window is small.**
  When the app window isn't wide/tall enough, the Save button scrolls out of view
  and can't be reached. Let the toolbar/controls wrap (or otherwise keep Save
  reachable) at small window sizes. _Added 2026-07-16 (M3c)._

- [ ] **Re-share correctly instead of inlining on overview save.** Overview column
  edits currently inline every `Shared`/`Ref` before encoding to avoid dangling
  refs (`RefBeforeStore`), which produces a valid but ~1.5x larger file that no
  longer matches what the EVE client would write. Revisit: re-derive a correct
  canonical `Shared`/`Ref` numbering after edits (encoder-side auto-dedup, sharing
  structurally-equal values in emit order) so the saved file matches the client's
  dedup. _Added 2026-07-16 (M3c)._

- [ ] **Group the file list by type (character vs account).** Split the sidebar
  file list into Character and Account (user) sections. Disambiguates the case
  where an account alias and a character name are identical. _Added 2026-07-16 (M3c)._

- [ ] **Verify (or migrate) legacy overview editing.** The legacy `tabsettings`
  overview format was never live-tested by the developer. Confirm editing works on
  a real legacy file; simplest safe option may be to upgrade a legacy file to the
  modern `tabsettings_new` shape when it's edited. _Added 2026-07-16 (M3c)._

- [ ] **Drop the recent-sibling-writes save warning.** On save, when other files
  in the same profile were modified in the last 5 minutes, the "Saved" dialog
  appends a warning that the EVE client may be running and could overwrite the
  changes on logout (`recent_sibling_writes`). Remove that warning from the save
  flow (the backup already protects against loss). _Added 2026-07-16 (M3c)._

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
