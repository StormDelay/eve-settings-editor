# Live verification plan

Everything the editor has shipped or is about to ship that has **never been
proven against a running EVE client**, arranged to cost as few client launches
and as few manual steps as possible.

Eight slices merged without their live smoke. This is the plan to clear all of
them at once rather than one relog at a time.

---

## 1. The cost model

Only two things are expensive, and neither is the checking:

| Step | Cost | Notes |
|---|---|---|
| Launching the client and logging in | minutes | the unit to minimise |
| Hopping to another character at char-select | seconds | **re-reads that character's file** |
| Editor edit + save | seconds | offline, unlimited |
| Reading a file back | seconds, scripted | see §3 |

Three consequences drive the whole plan:

1. **EVE writes settings on logout, and reads them on login.** Every capture is
   *act → log out → read*. Every editor-written change must be saved while that
   character is **logged out**, or the client overwrites it on its way out.
2. **Character-scoped state multiplies for free.** Logging out to character
   select writes the character you left and reads the one you enter. So *N*
   variants staged on *N* characters cost **one launch**, not *N*.
   Account-scoped state (`core_user_*`) does not: it needs a second account,
   i.e. a second client — run them side by side, that is still one launch event.
3. **Each login does double duty.** Observe everything staged offline, *then*
   perform the in-game actions the next capture needs, then quit. Never spend a
   login on only one direction.

Target: **two client sessions**, a third only to confirm fixes.

### What does *not* need the client

Manufacture edge-case starting states offline with the raw Tree view instead of
hunting for or creating them in-game — deleting `customCmds` gives you a
never-opened-the-keybinding-screen account; deleting `tabsettings_new` gives you
one of the 385/1925 accounts with no tab settings. Both are one-click in the
tree, and both are otherwise a trial-account hunt.

---

## 2. Test rig

| Slot | Purpose |
|---|---|
| **Account A**, chars **A1 A2 A3** | the main tour: char-scoped variants, batch source/target, the 3-window overview shape `[[0,1,2,3,4,7],[5],[6]]` (812 of 825 corpus accounts) |
| **Account B**, chars **B1 B2** | account-scoped variants: the second overview state, the destructive keybinding reset, the empty-`customCmds` target |

A2 and A3 exist to test the collateral/shared-account paths and batch copy; a
character on a *second* account is what makes account-scoped edits observable
independently. Run both clients at the same time.

**Before anything:** `tools\capture-diff.ps1 -Label baseline` (§3) and a copy of
the whole settings folder somewhere outside it. Every check below is reversible
from that. The editor's own backup chain also fires per save.

---

## 3. `tools/capture-diff.ps1`

Every capture is "which bytes moved". Doing that by eye in the Tree view is the
single largest source of manual steps in this plan, so it is scripted:
`capture-diff.ps1` snapshots the live directory under a label, decodes it, and
diffs it against a previous label.

```
tools\capture-diff.ps1 -Label baseline                        # prep, snapshot only
tools\capture-diff.ps1 -Label after-session-a -Against staged-1
tools\capture-diff.ps1 -Label X -Against Y -NoSnapshot        # re-read two old labels
```

Run it at **every** phase boundary — after each offline staging, after each
client quit — so each diff isolates exactly one actor, the editor or the client.
Snapshots land in `testdata/corpus/`, decoded text in `testdata/dumps/`, both
gitignored: they are real personal data and must never be committed. It reads
the live directory only through `sync-corpus.ps1`.

Two things it does that matter for how the read-out feels, both measured on the
historical `exp1` capture:

- **Only files whose bytes changed are decoded.** A live settings directory here
  is 586 files and decoded text runs ~20× the binary, so decoding everything
  would cost about a gigabyte a snapshot to show the handful of files a capture
  touched. exp1 decoded 5 of 586, for 3 MB. This matters: the drive has 1.7 GB
  free.
- **Dumps are inlined** (`bmdump dump-inline`). The client renumbers its
  shared-object slots on every write, so a raw dump buries the real change under
  hundreds of `shared[114]` → `shared[143]` lines. Inlining took exp1 from a
  wall of noise to 581 changed lines with zero slot churn. Use plain
  `bmdump dump <file>` by hand if you specifically want to see the sharing
  layout.

Even so, most of a diff is EVE re-stamping keys it rewrote — 467 of exp1's 581
changed lines were bare timestamps. To see only substance:

```
tools\capture-diff.ps1 -Label after-a -Against staged-1 | Select-String -NotMatch '^[+-]\s+\d+L\s*$'
```

That took exp1's read-out to 114 lines. Note the script exits 1 when the diff is
non-empty — git's own `diff --no-index` convention, not a failure.

This also gives you the plan's cross-cutting gate for free, see §7.

---

## 4. Session A

### Phase 1 — offline staging (client closed)

Stage every editor write that does not depend on something Session A will
discover. Save each character's file with that character logged out (all of
them are — the client is closed). Then
`capture-diff.ps1 -Label staged-1 -Against baseline` and eyeball that the diff
contains only what you meant to write.

**On A1 — neocom + layout + HUD (char-scoped):**
- Neocom: reorder two buttons, remove one, add one back from the dropdown
  (pick one sourced from the *catalog*, not from `Original`, so the catalog's
  `btnType`/`iconPath` are what gets tested).
- Layout: drag two windows flush using edge snapping; nudge a third with the
  arrow keys to an exact coordinate you write down; Shift-drag one window onto
  another to create a stack; drag a tab out of an existing stack.
- HUD: set `shipuialignleftoffset` to **0** and note where the canvas draws the
  ship HUD; set the fighter UI position to the canvas's top-left corner.
  These are the values Session A reads back against reality.
- Set the per-window **pinned** flag on one window.

**On A2 — the batch target:** nothing. It receives a copy in Phase 1 too:
batch-copy the **Layout** aspect A1 → A2 (carries the neocom bar) and the
**Keybinds** aspect A1 → A2 later, on account B.

**On A3 — untouched.** The control: whatever EVE does to a file the editor never
saved is the baseline for "did the client rewrite this on its own".

**On account A (account-scoped):**
- Keybindings: rebind three commands, one of them onto a combination another
  command already owns (the steal path).
- Overview: import a **community pack** through the editor (a downloaded one,
  not a self-export), on the 3-window account. Also export the account's current
  overview to `mine.yaml` before importing — Session A feeds that file back
  through EVE's own importer.
- Overview appearance: set two state background colours and a colortag, reorder
  the state priority list, tick two of the six appearance checkboxes.

**On account B (account-scoped, manufactured states):**
- Tree view: delete `customCmds` entirely → the never-opened-the-screen shape.
  Then batch-copy the **Keybinds** aspect A → B onto it. This is keybinds gate 2
  (a copied table carrying another account's timestamp, a shape the client has
  not been observed to read).
- Tree view: delete `tabsettings_new` → the zero-tab account. Then import a
  **tab-layout-only pack** (no `presets` section) through the editor. Two
  untested paths in one file: the zero-tab fallback, and tabs whose
  `overview`/`bracket` names the account has no preset for.

### Phase 2 — one launch, two clients

Order matters: **observe before you disturb**, and put the destructive actions
last.

**Client 1 — account A, character A1:**

1. **Screenshot at native resolution before touching anything.** This one image
   settles all three invented `HUD_NOMINAL` sizes (ship HUD 686×250, fighter UI
   400×120, badge 32×32) — measure them off the screenshot in pixels.
2. **Neocom:** the bar order matches the editor's; the removed button is gone;
   the added button *works* when clicked rather than rendering as a dead icon.
   Check a folder button (Inventory) still expands to its child.
3. **Layout:** the two snapped windows are flush; the nudged window sits at the
   coordinate you wrote down; the created stack is a real tabbed stack; the
   dragged-out tab is a free window; the pinned window is pinned.
4. **HUD conventions — the two-point capture.** One reading cannot separate an
   origin from a sign; two can. For the ship HUD, note where offset `0` actually
   put it (centred → centre-relative, hard left → left-relative), then drag it to
   a second, measurable position. For the fighter UI, note where the top-left
   value landed it, then drag it hard into the **screen's top-left corner** — a
   stored `(0,0)`ish value means the tuple is a top-left corner, a stored
   half-a-panel value means it is a centre. Repeat the corner drag for the
   notification badge. Screenshot after each drag.
5. **Chat channel ids — the one thing no test can settle.** Open a **named
   standing channel** (Local, or an alliance channel) *and* a **private
   conversation**; they may key differently. The editor derived
   `chatchannel_<channelKey>` from `ui → chatchannels`; if a real id carries a
   kind segment the key does not, every chat window silently keeps its derived
   name with no error anywhere.
6. **Overview — observe our pack import** (tabs, presets, colours, in-space ship
   labels, the appearance checkboxes and state colours from Phase 1) and
   screenshot, **before** the next step overwrites it.
7. **Overview — EVE's own importer.** Feed `mine.yaml` (our export) to Overview
   Settings → Import. If EVE accepts it, immediately **export from EVE** to
   `eve-export.yaml`. Diffing those two offline is what tells you whether EVE
   round-trips what we wrote or quietly normalises it — and it settles both open
   format questions: which internal boolean `applyOnlyToShips` maps to, and
   whether a current client emits suffixed (`backgroundStates2`) or unsuffixed
   state-list names. Import a pack with **more** tabs than the account and one
   with **fewer**, and note whether EVE renders a duplicated tab and what it does
   with a secondary window left empty. Import a pack whose ship labels carry
   **colours** (the C2 regression).
8. **Keybindings:** the three rebinds are live and the steal took the combination
   from its previous owner. Spot-check the labels against the in-game screen,
   particularly `OpenAgencyNew`, `OpenSkillQueueWindow`, `CmdPickPortrait0..3`
   and `ToggleCurrentSystemLocationWnd` — provenance-verified from localisation
   data but never seen in-game.
9. **Stack captures (destructive to the stack, so last):** drag the
   *second-to-last* window out of a stack, leaving one member. And note whether
   the phantom "window stack" frames the file carries (a real character had 8:
   `43 51 63 82 156 181 219 221`) are still there — the question is whether EVE
   re-creates them if deleted, which the *next* session answers.
10. **Dock/undock, if you want the per-environment work scoped:** open two known
    windows in station, undock, move one, dock again. Cheap here, expensive as
    its own trip.
11. **Log out to character select** — this writes A1.

**Client 1 — character A2:** the batch-copy target. Its neocom bar matches A1's,
its window layout matches, and its own `Original` was *not* overwritten. Log out.

**Client 1 — character A3:** the control. Nothing you did should show up here.
Log out, then **quit the client** (quit, not just log out — the account-scoped
file is written on the way out).

**Client 2 — account B, character B1:**

1. **Keybinds gate 2:** open the in-game keybinding screen. Does the copied
   table show, or does the client ignore a table carrying another account's
   timestamp? This is the whole reason the Keybinds batch category exists.
2. **Zero-tab overview:** the tab-layout-only pack's tabs render; note what EVE
   does with tabs whose preset names it does not know.
3. **Factory keybindings capture — destructive, do it last.** In the keybinding
   screen choose **Reset to default**. This is the only place EVE's factory
   bindings exist; `app/src/lib/data/command-defaults.json` ships empty because
   of it, which is why the Keybinds view's Default column and per-row reset are
   dead.
4. Quit the client.

### Phase 3 — offline

```
tools\capture-diff.ps1 -Label after-a -Against staged-1
```

Read out, in this order:

1. **The global gate first** (§7) — did EVE keep our values, or rewrite them?
2. The HUD numbers against your screenshots → settle or correct the three
   conventions.
3. The chat window ids against `ui → chatchannels`.
4. `stacksWindows` / `preferredIdxInStack3` for the one-member stack.
5. The factory keybinding table out of B's `core_user_*.dat` → generate
   `command-defaults.json`.
6. `git diff --no-index mine.yaml eve-export.yaml`.

---

## 5. Session B

Everything Session A discovered turns into code; Session B proves the code.
Stage offline exactly as in Phase 1 — `capture-diff.ps1 -Label staged-2` — then
one launch:

- Every convention Session A corrected, re-observed: write a HUD value from the
  editor, log in, confirm it lands where the canvas drew it. If a convention was
  wrong, this is the session that proves the fix, and it is the only reason
  Session B is not optional.
- The `command-defaults.json` you generated: the Default column populates and
  per-row reset restores the real factory binding.
- **Orphaned stack frames:** delete them from A1's file offline, log in, log out,
  and see whether EVE re-creates them. If it does, the "offer to delete orphaned
  frames" task is dead and can be closed rather than built.
- Anything Session A failed.
- The second round of overview-pack variants, now that the first round showed
  which ones matter.

## 6. Session C

Only if Session B found something. Do not plan for it; plan the fixes so it is
not needed.

---

## 7. The cross-cutting gate

Run after **every** session, on every file, before looking at any individual
feature. It is one read of the capture diff:

- **Did EVE accept the file at all?** A rejected settings file does not error —
  the client silently falls back to defaults. Any character whose whole layout
  or overview reverted means the codec, the reshare pass or a structural edit
  produced something the client would not read, and that outranks every other
  finding in this document.
- **Did EVE keep our values, or rewrite them?** A key the client rewrote on its
  own way out (rather than leaving as we wrote it) means the editor wrote a
  *legal but non-canonical* shape. Character A3, which the editor never touched,
  is the control for what the client rewrites unprompted.
- **Did anything we did not touch change?** A structural edit that reshuffles an
  unrelated subtree is the failure mode the `reshare` pass is most likely to
  produce, and no automated test can see it.

---

## 8. Item index

Nothing in the sessions above is optional; this table is the check that nothing
was dropped. "Source" is where the outstanding item is currently recorded.

| # | What | Ships in | Where | Source |
|---|---|---|---|---|
| 1 | Neocom reorder / remove / add / reset accepted in-game | unreleased branch | A / A1 | spec §8 |
| 2 | Neocom rides a Layout batch copy | unreleased branch | A / A2 | spec §8 |
| 3 | Chat window id ↔ `chatchannels` key (named channel *and* private convo) | 0.19.0 | A / A1 | ledger |
| 4 | An editor-minted stack still reads `Window stack · N` | 0.19.0 | A / A1 | ledger |
| 5 | Stack frame label layout with a long name | 0.19.0 | A / A1 | ledger |
| 6 | `preferences.json` round trip, corrupt → `.bad`, two rapid toggles | 0.19.0 | offline | ledger |
| 7 | `overrideCount()` scope across characters | 0.19.0 | offline | ledger |
| 8 | Edge snapping lands windows flush | 0.17.0 | A / A1 | — |
| 9 | Arrow-key nudge lands on the exact coordinate | 0.17.0 | A / A1 | — |
| 10 | Shift-drag creates a real stack; tab drag-out frees a window | unreleased (master) | A / A1 | — |
| 11 | What a one-member stack does | unreleased (master) | A / A1 | ledger |
| 12 | Whether EVE re-creates deleted orphan frames | n/a (scoping) | B | ledger |
| 13 | Keybind rebind honoured, not reverted | 0.18.0 | A / A1 | ledger |
| 14 | Keybind table copied onto an empty `customCmds` | 0.18.0 | A / B1 | ledger |
| 15 | Keybind label spot-checks | 0.18.0 | A / A1 | ledger |
| 16 | Factory keybindings capture | 0.18.0 | A / B1 | ledger |
| 17 | Ship HUD offset: centre-relative? sign? | 0.15.0 | A / A1 | ledger + format-notes |
| 18 | Fighter/badge tuples: top-left or centre? | 0.15.0 | A / A1 | ledger + format-notes |
| 19 | `HUD_NOMINAL` sizes | 0.15.0 | A / A1 | ledger + format-notes |
| 20 | The nine HUD fields write and take effect | 0.15.0 | A / A1 + B1 | — |
| 21 | Account-scoped HUD fields hit every character on the account | 0.15.0 | A / A2 | — |
| 22 | Pack import: community pack, 3-window account | 0.14.0 | A / A1 | ledger |
| 23 | Pack import: zero-tab account | 0.14.0 | A / B1 | ledger |
| 24 | Pack import: tab-layout-only (no presets) | 0.14.0 | A / B1 | ledger |
| 25 | Pack import: more tabs / fewer tabs than the account | 0.14.0 | A / A1 | ledger |
| 26 | Pack import: ship labels with colours (C2 regression) | 0.14.0 | A / A1 | ledger |
| 27 | EVE's importer accepts our export | 0.14.0 | A / A1 | ledger |
| 28 | Export → EVE import → EVE export → diff | 0.14.0 | A / offline | ledger |
| 29 | `applyOnlyToShips` → which internal boolean | 0.14.0 | A / offline (#28) | ledger |
| 30 | Suffixed vs unsuffixed state-list names in a current export | 0.14.0 | A / offline (#28) | ledger |
| 31 | Tab order inside a window survives a pack round trip | 0.14.0 | A / offline (#28) | ledger |
| 32 | State colours, colortags, priority order render as set | 0.13.0 | A / A1 | partly run |
| 33 | The six appearance checkboxes | 0.13.0 | A / A1 | partly run |
| 34 | Preset exceptions (Show / Hide / Always show) | 0.13.0 | A / A1 | partly run |
| 35 | Per-environment window mapping (dock/undock diff) | n/a (scoping) | A / A1 | ledger |
| 36 | **The client accepts every file the editor wrote** | all | every session | §7 |

Items 6 and 7 need no client at all — do them offline while the client is
launching.

---

## 9. What the results feed

| Finding | Lands in |
|---|---|
| HUD conventions | `app/src/lib/layout.ts` (`HUD_NOMINAL`, and each convention **with its inverse** — `shipOffsetFromX`, `hudPointFromRect`), the `layout.test.ts` round-trip cases that pin them, and `docs/format-notes.md` §"HUD anchors" (delete the "NOT yet confirmed" heading, or correct it) |
| Chat id link | `docs/settings-field-reference.md` (state the link rather than leaving it inferred), `windowLabels.ts` if the derivation is wrong |
| Factory keybindings | `app/src/lib/data/command-defaults.json` |
| Pack format answers | `overview_pack.rs` (`applyOnlyToShips`, the `USER_SETTINGS` identity map that exists only to be `debug_assert`ed), `docs/format-notes.md` |
| One-member stack, orphan frames | either close the two ledger tasks or implement them |
| Everything confirmed | `CHANGELOG.md` — 0.14.0's "treat this release's overview features as unstable" and 0.15.0's unconfirmed-conventions note both come out |
| Everything done | `docs/small-tasks.md` — six open items close |
