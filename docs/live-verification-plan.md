# Live verification plan

Everything the editor has shipped or is about to ship that has **never been
proven against a running EVE client**, arranged to cost as few client launches
and as few manual steps as possible.

Nine slices merged without their live smoke, spanning 0.13.0 through 0.21.0.
This is the plan to clear all of them at once rather than one relog at a time.

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

**Deleting `tabsettings_new` is not enough on an account that also carries the
legacy `tabsettings`.** `overview.rs::migrate_legacy_overview` renames legacy →
modern on the next overview edit, but *only when `tabsettings_new` is absent* —
precisely the state the deletion creates. So the next edit silently resurrects
the legacy table under the modern name and the zero-tab path never runs. Account
B had both keys and this is exactly what happened on the first attempt. **Delete
both keys**, and confirm the account has neither before importing.

---

## 2. Test rig

| Slot | Who | Purpose |
|---|---|---|
| **A** | `7214485` StormDelay | |
| **A1** | Storm Delay `93622368` | the main subject: neocom, layout, HUD, chat, overview, keybinds |
| **A2** | StormDelay `1985569356` | batch-copy target |
| **A3** | Sturm Dulu `96373105` | **untouched control** — see §7 |
| **B** | `13375506` stormdelay7 | |
| **B1** | Holy Storm `96821229` | account-scoped variants: the second overview state, the destructive keybinding reset, the empty-`customCmds` target |
| **B2** | Ranlib `2113799945` | settings-preset target |
| **B3** | Trucmachin Padecain `2112960398` | fresh-install target for the `Everything` preset — its settings files are moved aside before Session A so EVE creates virgin ones |

Two accounts, because a character on a *second* account is what makes
account-scoped edits (`core_user_*`: keybinds, all of overview, autofill)
observable independently — a second account means a second client, so run them
side by side. Six characters, because each one is a free variant slot (§1) and
they are what keeps the plan to two launches.

**The three-window requirement is met** — the pack tests are worth little on a
hand-made two-window account, and a scan of all 19 live TQ account files found
none: every configured account here already carries `[[0,1,2,3,4,7],…]`, 17 of
them with windows 5 and 6 in the reverse order to the corpus-canonical
`[[0,1,2,3,4,7],[5],[6]]`. Account A is the one that matches the corpus exactly.
Account B is a **four**-window account (`…,[5],[6],[8]`), which is a free extra
variant for the pack tests, and account `32945923` has no
`tabsByWindowInstanceID` at all — a real never-configured account, should the
manufactured zero-tab state in Phase 1 ever be doubted.

To re-run that scan: decode each `core_user_*.dat` with `bmdump dump-inline` and
read `overview → tabsByWindowInstanceID`.

### The overview packs

Four files in `Documents\EVE\Overview\`, one per pack-import item. The account
has **8 tabs**, which is what "more"/"fewer" are measured against.

| File | Tabs | Serves |
|---|---|---|
| `zs_full_v10.06.09.yaml` | 7 | item 22 (community pack) and item 26 — its `alliance` ship label carries `color: [1.0, 1.0, 0.4]`, so the C2 regression rides along for free |
| `fenris_default_v24.01.yaml` | 5 | item 25, the *fewer* half — a second author, so a second set of shapes the editor did not write |
| `derived_tabs_only_no_presets.yaml` | 5 | item 24 — Fenris with its `presets` section deleted |
| `derived_zs_more_tabs.yaml` | 11 | item 25, the *more* half — Z-S with four cloned tabs appended |

The two `derived_` files are built by `derive-packs.py`; no published pack has
more tabs than the account, and none ships without presets. All three published
packs emit **unsuffixed** `backgroundStates`, which is half of item 30 —
the other half is what a current *client* emits, which only §4 Phase 2 step 7
can answer.

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
  touched. exp1 decoded 5 of 586, for 3 MB. A snapshot itself is ~50 MB, so the
  budget is the decode, not the copy.
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

**Settings presets (0.20.0) — all offline, all on B2:**
- Save a **Layout-only** preset from A1, then apply it to **B2** through the
  batch view's new Preset source. The preview must write B2's *character* file
  and nothing else; the diff after saving is the check, since B2 shares account
  B's user file with B1 and cannot show account-level non-interference in-game.
- Open that preset, move one window in it, save, and **re-apply** to B2. This is
  the claim that a preset is genuinely editable rather than a frozen capture,
  and the second apply is what proves the edit reached a real file.
- Save an **Autofill-only** preset and open it: Overview, Layout, Keybinds and
  the tree must show honest empty states rather than erroring. A pruned preset
  is a document with whole sections missing, a shape no EVE-written file has
  ever had, so this is genuinely new ground for every projection. **Passed** —
  the empty states are honest ("This account file has no overview tabs"), no
  projection threw.
  ~~Then build an overview preset inside it **from nothing** — the slice-2b
  minting path with no `overview` container to start from — and apply it to
  B2.~~ **P5 is blocked and drops out of this session.** Nothing in the codebase
  creates an `overview` container, so a pruned preset that has none can neither
  build one nor import one: `overview_mut` is the only way in for both the tab
  editor and `apply_pack`, and it returns `NoOverview` — pinned by
  `applying_a_pack_to_a_file_with_no_overview_container_errors`. Confirmed
  against the real preset on disk: its `user.dat` carries `ui` only and its
  `char.dat` is a 7-byte empty document. Recorded in `small-tasks.md`; it needs
  the minimum-viable-container question answered before it can be code. Nothing
  else in Block 6 depends on it.
- **Export** a preset to `.evepreset`, **import** it back under a new name, and
  apply the copy to B2 as well. The byte-identical round trip is already unit
  tested; what is not is that the reimported copy behaves the same in-game.
- Delete a preset's `preset.json` and confirm the preset still lists, offered as
  its **pruned aspects only** — never as `Everything`. The safe direction is
  "fewer aspects offered"; the unsafe one overwrites a whole character file with
  a three-key document.
- Confirm the per-column **width field is editable with a preset open**
  (design §5.1: it was gated on `charId`, which a preset does not have).
- **Move B3's settings files aside.** EVE recreates them at its first login in
  Session A, which is what gives Session B a genuine brand-new-install target.

### What Phase 1 actually staged on A1

Read out of the `baseline → staged-1b` diff, so these are what the *file* holds,
not what anyone remembers doing. Phase 2 checks the client against this table.

| Item | Key | Expected in-game |
|---|---|---|
| 9 | `directionalScannerWindow` | at **x 40, y 497, 385×519** — the arrow-key nudge target |
| 8 | same, vs `ChatWindowStack` | D-Scan's bottom is 497+519 = **1016**; the chat stack's top is **1016**, both left edges at **40**. Flush, zero gap |
| — | `pinnedWindows[directionalScannerWindow]` | `True` — pinned |
| 10 | stack **`1000`** | `fittingWnd` (idx 0) + `StructureBrowser` (idx 1) at 107,218,1327,676. Reads `Window stack · 2` |
| 10 | `chatchannel_corp` | dragged out of `ChatWindowStack`, free at **50,163**, 256×424 |
| 17 | `shipuialignleftoffset` | **0.0** — was **−189.0**, and a negative value cannot be an offset from the left edge of the screen |
| 18 | `fightersDetachedPosition` | **(0, 0)** — was (326, 54) |
| 1 | neocom | `corporation` gone; `ProjectDiscovery` added with `iconPath` `b""`; `contracts` added with `res:/ui/Texture/WindowIcons/contracts.png` |

The §7 gate on that diff: 79 windows moved, **all explained** — 74 are
`ChatWindowStack` members following their stack, 5 are the windows actually
touched. One window added (`1000`, the minted stack). No unrelated subtree
moved, which is the reshare failure mode §7 exists to catch.

**A2, after the Layout copy (item 2):** A2's `windows` went 296 → 380 and its
window set is now *identical* to A1's — 0 in A2 that are not in A1, 0 the other
way. The wholesale-replace semantic recorded in `format-notes.md` holds. A2's
neocom matches A1's twelve buttons, and **A2's own
`neocomButtonRawDataOriginal` was not touched** (same 14 buttons before and
after), so A2 keeps its own reset baseline. In-game, expect A2's layout and
neocom to match A1's, and — per that same note — expect EVE to re-create A2's
own chat/convo windows and any account-scoped overview window on next login.

### What Phase 1 staged on account A

One file changed, `core_user_7214485.dat`, and every changed section is inside
`overview` or `customCmds`. Nothing reached A1, A2 or A3's character files.

- **Keybind steal (item 13):** `CmdActivateHighPowerSlot4` took `68` from
  `CmdActivateHighPowerSlot3`, which became `None`. The previous owner is
  **unbound, not left holding a duplicate** — in-game, slot 3 must have no
  binding and slot 4 must fire on `68`.
- **Pack import (item 22):** `overviewProfilePresets` 10076 → 12118 lines,
  `tabsettings_new` 225 → 46, `shipLabels` 40 → 68, plus the column set.
- **`tabsByWindowInstanceID`: `[[0,1,2,3,4,7],[5],[6]]` → `[[0,1,2,3,4],[5],[6]]`.**
  The account had 8 tabs, Z-S has 7, so index 7 was dropped from window 0 and
  all three windows kept at least one tab. **Prediction for item 25:** Fenris
  has 5 tabs (0–4) while windows 1 and 2 expect indices 5 and 6, so importing it
  should leave two windows empty — the case Z-S does not reach.
- **`overviewProfilePresets_notSaved` emptied, 344 lines → 0.** Deliberate:
  `overview_pack.rs:526` drops the key, and the test at :1727 pins it ("a stale
  notSaved working copy must not survive a preset replacement"). It is a
  name-keyed working buffer whose keys would name presets that no longer exist.
- **Appearance:** `useSmallColorTags` and `useSmallText` flipped;
  `backgroundStates2`/`backgroundOrder2`/`flagStates2`/`flagOrder2`,
  `stateColors` and `stateBlinks` all moved.

### What Phase 1 staged on account B

- **Keybinds gate 2 (item 14) is staged and verified byte-for-byte.** After
  deleting `customCmds` and batch-copying the Keybinds aspect A → B, account B's
  table is *identical to account A's, timestamp included* — both carry
  `134295535613980310L`, which is A's. That is exactly the shape the gate exists
  to test. In-game: open B1's keybinding screen and see whether the client shows
  the copied table or ignores it as stale.
- **The zero-tab import (item 24) ran for real on the second attempt**, with no
  `tabsettings*` key of any kind present. It exposed two format bugs, both now
  fixed: the create-from-absent path wrote a bare payload where every container
  key is `(timestamp, payload)`, and — separately, on *every* pack import on
  *any* account — the two column keys had their wrapper stripped. See the
  CHANGELOG's Unreleased section.
- **Watch the minted timestamp in-game.** B's re-created `tabsettings_new` and
  `tabsByWindowInstanceID` carry `0L`, where every EVE-written key holds a real
  FILETIME (`134295…`). That is the convention `hud.rs` has always used when
  creating a key from nothing, so it is not new — but it has never been
  confirmed against a client either, and account B is now the first file to
  carry it into one. If EVE discards or resets those two keys while accepting
  everything else, a zero timestamp is the first suspect.
- **Three overview windows are left empty** (`[[0,1,2,3,4],[],[],[]]`) — the
  item 25 case Z-S could not reach, since account B defines four windows and the
  pack has five tabs.
- **Deleting `tabsettings_new` needed a code fix first.** The tree refused to
  remove any node whose subtree defined a shared object, and on a real file
  `tabsettings_new` is full of them — so the manufactured zero-tab state in §1
  was unreachable. `mutate::remove_entry` now inlines and reshares around such a
  removal (the pattern `neocom.rs` already used), and `projection.rs` no longer
  gates `removable` on it. Fixed mid-session, so **anything staged through the
  tree's delete after 2026-07-27 15:10 ran against patched code**, not 0.21.0.
  Nothing staged before that used the removal path.

### Phase 1 outcome — staged, 2026-07-27

**Phase 1 is complete. The label to diff Phase 2 against is `staged-7`**, not
`staged-1`: the staging ran as eight snapshots (`staged-1a` … `staged-7`) so
each block could be read in isolation.

| Item | Result |
|---|---|
| P1 Layout preset applies to another character | ✅ file-level |
| P2 a preset is editable, edit re-applies | ✅ backup chain fired inside the preset folder |
| P4 pruned preset shows honest empty states | ✅ no projection threw |
| P5 overview preset minted from nothing | ❌ **blocked — not implementable**, see above |
| P6 export → import → the copy behaves identically | ✅ three-way byte-identical: original preset, reimported copy, and what landed on B2 (4,877 lines each) |
| P7 column width editable with a preset open | ✅ |
| P8 missing `preset.json` offers pruned aspects only | ✅ `A1_everything_preset` carries both full files with no marker, and lists pruned |
| B3's files moved aside | ✅ gone from the live folder, 174 files snapshot |

Six bugs were found and four fixed before a single client launch: the raw
tree's shared-subtree delete refusal, the pack import stripping the
`(timestamp, …)` wrapper off two column keys on **every** import, the
create-from-absent paths writing bare payloads, and the `tabsettings_new`
deletion recipe in §1 being incomplete. The two not fixed (P5's missing
minting path, `tabsettings2` being unread) are recorded in `small-tasks.md`
along with seven usability findings.

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
   **Look hardest at the button added from `Original`.** Staging found that
   every button in A1's `neocomButtonRawDataOriginal` carries no `iconPath` key
   at all, where every button on the live bar has one. `read_button` maps the
   absent key to `""` (`neocom.rs:102`) and `addableButtons` lets `Original`
   overwrite the catalog entry (`neocom.ts:39`), so adding one of those 14
   writes a four-key button whose `iconPath` is the empty string — a shape no
   client has been observed to write. **Absent key is not the empty string:**
   EVE reads its own three-key buttons fine, but whether it treats `""` as a
   literal path (blank icon) or as falsy (falls back by id) is the question, and
   only the client can answer it. Add one button from `Original` and one from
   the catalog, and compare how the two render.
   `reset()` is a different path — it copies `Original`'s raw values verbatim
   (`neocom.rs:260`), so it writes genuine three-key buttons and is unaffected.
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
   **Then feed `zs_full_v10.06.09.yaml` to EVE's importer too — this is the only
   way to grow the colour palette.** Staging the pack through the editor warned
   *"unknown colour name 'black' — left at EVE's default"*: the pack sets
   `flag_48: black`, and `PALETTE` (`overview_pack.rs:260`) holds only five
   names — `blue, darkBlue, orange, red, white`. That table was harvested from
   this corpus by joining `overview→restoreData→data` (the pack verbatim, as EVE
   stored it) against `overview→stateColors` (the RGBA EVE derived), so it only
   ever contained names an account here had already imported. **`restoreData` is
   written by EVE and never by us** — the editor's own importer does not produce
   it — so no amount of offline work can add a name. One import through EVE's
   screen makes the client write both halves of the join, and
   `cargo run -p settings-model --bin pack_palette -- <corpus-dir>` on the
   post-logout capture yields `black`'s RGBA. Skipping an unknown name rather
   than approximating it is correct and must stay that way; the fix is a bigger
   palette, not a fuzzier match.
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
4. Log out to character select.

**Client 2 — character B2, the preset target:**

1. The windows land where the **Layout-only** preset had them, including the one
   moved by editing the preset itself and re-applying.
2. ~~The overview preset **minted from nothing** inside the Autofill-only preset
   is present and usable.~~ Dropped — P5 is blocked, see Phase 1.
3. The **reimported** `.evepreset` copy behaves identically to the original.
4. Log out.

**Client 2 — character B3:** log in once and straight back out. Its settings
files were moved aside in Phase 1, so this is EVE creating virgin ones — the
brand-new-install target Session B pours the `Everything` preset onto. Nothing
to observe here beyond the client starting normally.

Quit the client.

### Session A results — Phase 2, 2026-07-27

| # | Item | Result |
|---|---|---|
| 36 | **The client accepts every file we wrote** | ✅ 380→383 windows on A1, 1 moved / 3 added / **0 removed**, all explained by in-game actions |
| 3 | Chat window id ↔ `chatchannels` | ✅ `chatchannel_` + the tuple's first element, same structure for named and private |
| 13 | Keybind rebind honoured | ✅ slot 4 on D (VK 68), slot 3 unbound |
| 14 | **Keybind table copied onto an empty `customCmds`** | ✅ **the client reads it** — B1's screen shows account A's table verbatim (slot 3 None, 4 D, 5 F2, 6 F1) despite carrying A's timestamp. This is what the Keybinds batch category exists for |
| 15 | Keybind labels | ⚠️ 2 of 4 spot-checks wrong, corrected in `command-names.json` |
| 16 | Factory keybindings | ❌ **not capturable from a file** — "Reset to default" writes `customCmds: {}`, an *empty* dict. EVE never persists factory bindings; `customCmds` holds only overrides. **The UI stays as it is** and gets filled by transcribing the in-game keybinding screen from screenshots: `defaultFor` returns null per command, so each entry added lights up its own Default cell and reset button and partial data is harmless |
| 17 | Ship HUD offset | ✅ centre-relative, negative = left, anchors the HUD's own centre |
| 18 | Fighter UI tuple | ✅ **x is the panel's left edge in absolute screen px** (stored 839 vs 838 measured). y is negative-up with an origin ~234px below the screen top |
| 22 | Community pack, 3-window account | ✅ Z-S tabs rendered |
| 24 | Tab-layout-only pack on a zero-tab account | ✅ all 5 tabs rendered; EVE **keeps the dangling preset name** (tooltip reads `Filter: General: General`) rather than erroring |
| 25 | More tabs than the account | ✅ the duplicated tabs render — `dup7`…`dup10` all appeared |
| 4, 10 | **Editor-minted stack** | ❌ **persisted but not rendered.** `1000` survives the login and is structurally identical to EVE's own numeric stacks (same sections; `lockedWindows` is present on only 2 of 6 real ones). The two members opened unstacked, so the likeliest cause is that `fittingWnd` and `StructureBrowser` are not stackable *with each other* — retest with two windows known to tab together |
| 26b | Palette harvest | ❌ **not possible this way.** EVE's own importer **discards** `flag_*` colour entries: after it ate Z-S, `flag_48` is absent from the file entirely and `stateColors` holds only `background` surfaces. Our skipping the unknown `black` matched the client's own behaviour. To grow the palette, use a pack setting a colour on a **background** state (`overview-states.json` notes `black` is the built-in for state 66) |
| 27 | EVE's importer accepts our export | ✅ round trip closed |
| 30 | Suffixed vs unsuffixed state names | ✅ **unsuffixed** — EVE emits `backgroundStates`, `flagStates` |

Also observed, and not in the plan: **a chat tab dragged out does not survive a login** —
`chatchannel_corp` went straight back into `ChatWindowStack`, consistent with
`format-notes.md`'s "chat windows follow the character's runtime state". Item 10's
drag-out half needs a *non-chat* window to be a valid test. And **the neocom renders
differently docked vs in space** (`small-tasks.md`).

**B3's virgin file exists** — 2,490 bytes, 12 top-level keys, written by EVE at its
first login. Session B's P3 target is ready.

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
7. B2's diff: the Layout-only apply touched B2's **character file only**.
8. B3's virgin files — confirm they exist and are what a fresh install looks
   like, then stage the `Everything` preset onto them for Session B.

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
- **The fresh-install case (P3), on B3.** Pour an `Everything` preset — taken
  from a fully configured character — onto the virgin files EVE created in
  Session A, then log in. The client should come up configured: layout,
  overview, keybinds, the lot. This is the strongest single test in the plan,
  because `Everything` is a complete copy of both files rather than a splice,
  so it is the whole save chain, the reshare pass and every structural edit
  landing on a client that has no prior state to fall back on.
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
| 1 | Neocom reorder / remove / add / reset accepted in-game | 0.21.0 | A / A1 | spec §8 |
| 2 | Neocom rides a Layout batch copy | 0.21.0 | A / A2 | spec §8 |
| 3 | Chat window id ↔ `chatchannels` key (named channel *and* private convo) | 0.19.0 | A / A1 | ledger |
| 4 | An editor-minted stack still reads `Window stack · N` | 0.19.0 | A / A1 | ledger |
| 5 | Stack frame label layout with a long name | 0.19.0 | A / A1 | ledger |
| 6 | `preferences.json` round trip, corrupt → `.bad`, two rapid toggles | 0.19.0 | offline | ledger |
| 7 | `overrideCount()` scope across characters | 0.19.0 | offline | ledger |
| 8 | Edge snapping lands windows flush | 0.17.0 | A / A1 | — |
| 9 | Arrow-key nudge lands on the exact coordinate | 0.17.0 | A / A1 | — |
| 10 | Shift-drag creates a real stack; tab drag-out frees a window | 0.21.0 | A / A1 | — |
| 11 | What a one-member stack does | 0.21.0 | A / A1 | ledger |
| 12 | Whether EVE re-creates deleted orphan frames | n/a (scoping) | B | ledger |
| 13 | Keybind rebind honoured, not reverted | 0.18.0 | A / A1 | ledger |
| 14 | Keybind table copied onto an empty `customCmds` | 0.18.0 | A / B1 | ledger |
| 15 | Keybind label spot-checks | 0.18.0 | A / A1 | ledger |
| 16 | ~~Factory keybindings capture~~ **IMPOSSIBLE — see below** | 0.18.0 | done | ledger |
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
| 26b | Palette: harvest `black` (and any sibling) via EVE's own importer | 0.14.0 | A / A1 | staging, 2026-07-27 |
| 27 | EVE's importer accepts our export | 0.14.0 | A / A1 | ledger |
| 28 | Export → EVE import → EVE export → diff | 0.14.0 | A / offline | ledger |
| 29 | `applyOnlyToShips` → which internal boolean | 0.14.0 | A / offline (#28) | ledger |
| 30 | Suffixed vs unsuffixed state-list names in a current export | 0.14.0 | A / offline (#28) | ledger |
| 31 | Tab order inside a window survives a pack round trip | 0.14.0 | A / offline (#28) | ledger |
| 32 | State colours, colortags, priority order render as set | 0.13.0 | A / A1 | partly run |
| 33 | The six appearance checkboxes | 0.13.0 | A / A1 | partly run |
| 34 | Preset exceptions (Show / Hide / Always show) | 0.13.0 | A / A1 | partly run |
| 35 | Per-environment window mapping (dock/undock diff) | n/a (scoping) | A / A1 | ledger |
| P1 | Layout-only preset applies to another character | 0.20.0 | A / B2 | preset spec §12.1 |
| P2 | A preset is editable: edit, re-apply, edit landed | 0.20.0 | A / B2 | preset spec §12.2 |
| P3 | `Everything` preset onto a brand-new install | 0.20.0 | **B / B3** | preset spec §12.3 |
| P4 | Pruned preset: honest empty states in every editor | 0.20.0 | offline | preset spec §12.4 |
| P5 | ~~Overview preset minted from nothing inside a preset~~ **BLOCKED — not implementable** | 0.20.0 | — | preset spec §12.4 |
| P6 | Export → import → the copy behaves identically | 0.20.0 | A / B2 | preset spec §12.5 |
| P7 | Column width editable with a preset open | 0.20.0 | offline | preset spec §12.6, §5.1 |
| P8 | Missing `preset.json` offers pruned aspects, never `Everything` | 0.20.0 | offline | preset spec §3.1 |
| 36 | **The client accepts every file the editor wrote** | all | every session | §7 |

Items 6, 7, P4, P7 and P8 need no client at all — do them offline while the
client is launching.

P3 is the only item that requires Session B rather than A, because a
brand-new-install target has to be created by EVE (Session A) before a preset
can be poured onto it (Session B).

---

## 9. What the results feed

| Finding | Lands in |
|---|---|
| HUD conventions | `app/src/lib/layout.ts` (`HUD_NOMINAL`, and each convention **with its inverse** — `shipOffsetFromX`, `hudPointFromRect`), the `layout.test.ts` round-trip cases that pin them, and `docs/format-notes.md` §"HUD anchors" (delete the "NOT yet confirmed" heading, or correct it) |
| Chat id link | `docs/settings-field-reference.md` (state the link rather than leaving it inferred), `windowLabels.ts` if the derivation is wrong |
| Factory keybindings | `app/src/lib/data/command-defaults.json` |
| Pack format answers | `overview_pack.rs` (`applyOnlyToShips`, the `USER_SETTINGS` identity map that exists only to be `debug_assert`ed), `docs/format-notes.md` |
| Colour palette | `overview_pack.rs` `PALETTE` — `black` at minimum, plus any other name the harvest turns up. Re-run `pack_palette` against the post-Session-A corpus rather than hand-editing the table |
| One-member stack, orphan frames | either close the two ledger tasks or implement them |
| Preset findings | `app/src-tauri/src/presets.rs` and the batch planner; the design spec's §12 checklist gets ticked off in place |
| Everything confirmed | `CHANGELOG.md` — 0.14.0's "treat this release's overview features as unstable" and 0.15.0's unconfirmed-conventions note both come out |
| Everything done | `docs/small-tasks.md` — six open items close |
