# Live verification — Session C (2026-07-28)

Everything shipped on `live-verification-session-a` that **writes to a settings
file and has never been run against a running client**, plus the one measurement
the HUD footprint work left inferred. Arranged for **one launch event**.

Sessions A and B settled the read-side conventions. This session is about the
write side: two features that change a real file, and one number.

---

## 0. What is actually at risk

| Check | Writes what | If it is wrong |
|---|---|---|
| **W** Windowless opt-in | `tabsByWindowInstanceID` on an account | **Overview tabs disappear in game.** The whole design refuses rather than write a partial mapping, but that has never faced a client. |
| **O** Orphan-frame delete | Removes window ids from 10 dicts on a character | A window loses its position/flags. The *deletion* was hand-verified 2026-07-28; the **button** has not been. |
| **H** HUD second offset | Nothing — the client writes, we read | Nothing. Pure measurement. |
| **N** Neocom docked vs space | Nothing — screenshots only | Nothing. |

**W is the one to be careful with**, so it runs on `stormdelayghost` (account
`32945923`, one character) rather than the main.

## 1. The rig

| Account | Alias | Character | Checks |
|---|---|---|---|
| `32945923` | stormdelayghost | `2124209999` | **W** |
| `7214485` | StormDelay | `93622368` Storm Delay | **O**, **H**, **N** |

Both accounts are windowless (no `tabsByWindowInstanceID`); the profile is
`g_eve_shared_cache_sharedcache_tq_tranquility\settings_Default`.

Storm Delay carries orphan frames `156 181 219 221` (4), and its current
`shipuialignleftoffset` is **-642.0** — the value the 2026-07-28 measurements
were taken at.

The two accounts are separate logins. Run the two clients **side by side**: that
is still one launch event, and it is the difference between one session and two.

## 2. Order of operations

The house rules that decide the order:

1. **EVE writes settings on logout and reads them on login.** So every editor
   change must be saved while that character is **logged out**, and every
   observation of what the client did needs a **logout** before the file is read.
2. Capture before and after each direction, or a diff proves nothing.

---

## 3. Phase 0 — offline, with EVE fully closed

- [ ] **0.1 Quit EVE completely.** Not character select — the process. A running
  client overwrites on exit and would silently undo everything staged here.

- [ ] **0.2 Back up the live profile.** Two of these checks write. Copy the whole
  folder somewhere outside the EVE tree (not beside it — a
  `settings_Default - backup` sibling gets swept into the corpus by
  `sync-corpus.ps1`'s default glob and lands as dozens of spurious files):

```powershell
Copy-Item -Recurse "$env:LOCALAPPDATA\CCP\EVE\g_eve_shared_cache_sharedcache_tq_tranquility\settings_Default" "$env:USERPROFILE\eve-settings-backup-2026-07-28"
```

- [ ] **0.3 Capture the baseline.**

```powershell
pwsh tools\sync-corpus.ps1 -Label c-baseline -Settings settings_Default
```

- [ ] **0.4 Stage O — delete Storm Delay's orphan frames.** Open
  `core_char_93622368.dat` in the editor, go to the Layout view, and **read the
  banner before clicking**: it should offer **4** empty stack frames. Record the
  number it actually says — the point of this check is that the number the UI
  names is the number that disappears. Click *Delete them*, confirm, **save**.

- [ ] **0.5 Stage W — give the ghost account per-window tabs.** Open the account
  file for `32945923` (character `2124209999`), go to the Overview view.
  - Confirm the notice reads "Tabs aren't assigned to specific overview
    windows on this account…".
  - Click *Set up per-window tabs*, read the confirm, accept, **save**.

  **The account's state was read out of the file before staging, so this check
  does not depend on transcribing anything.** It carries **5 tabs**, indices
  `0..4`:

  | Index | Name |
  |---|---|
  | 0 | General |
  | 1 | Targets |
  | 2 | Mining |
  | 3 | WarpTo |
  | 4 | All |

  So the opt-in must write exactly `tabsByWindowInstanceID = [[0, 1, 2, 3, 4]]`
  — one window, all five, ascending. Any shorter list is the failure the design
  exists to prevent, and it would show up in game as a missing tab.

  Five tabs is what makes this a real test: a single-tab account could not tell
  "lists every tab" apart from "lists the first tab".

- [ ] **0.6 Capture what was staged.**

```powershell
pwsh tools\sync-corpus.ps1 -Label c-staged -Settings settings_Default
```

---

## 4. Phase 1 — the client

### Client 1: stormdelayghost (`32945923`) → check W

- [ ] **1.1** Log in character `2124209999`.
- [ ] **W1 — every tab survived.** Open the overview. Compare against the list
  from 0.5: **same tabs, same names, same order.** A missing tab is the failure
  this whole design exists to prevent — if one is gone, stop, note which, and
  restore from the 0.2 backup.
- [ ] **W2 — the overview still populates.** It should show entries normally, not
  an empty list.
- [ ] **W3 — the tabs are usable.** Click through them; each should switch
  normally.
- [ ] **1.2** Log out to character select, then **quit the client** so it writes.

### Client 2: StormDelay (`7214485`) → checks O, H, N

- [ ] **1.3** Log in `93622368` Storm Delay.

- [ ] **O1 — nothing went missing.** The 4 deleted frames were empty containers,
  so *nothing visible* should have changed: every window you had is still there,
  in place. Specifically look for a window that lost its position or came back
  at a default spot.
- [ ] **O2 — no phantom rectangles.** No empty "Window stack" frames.

- [ ] **N1 — neocom, docked.** While docked, take a **full-screen PNG** (not
  JPEG). Name it `neocom_docked.png`.
- [ ] **N2 — neocom, in space.** Undock. Take the same shot as
  `neocom_space.png`. This settles whether the bar the editor models is the
  docked one, per the open ledger entry.

- [ ] **H1 — drag the ship HUD.** In space, drag the ship HUD **sideways by a
  large, obvious amount** — as far left or right as it will go is ideal. The
  bigger the move, the smaller the measurement error. Take a full-screen PNG as
  `hud_moved.png`.
  - This is the whole check: at the new offset, the capacitor wheel's centre must
    still land on `reference_w/2 + offset`, **and the column of round ship-control
    buttons on the HUD's left must have moved with it.** That second half is the
    only inferred number in `HUD_NOMINAL` (the 148px left extension).
- [ ] **H2 — bottom-aligned, if you can.** If there is a setting to put the ship
  HUD at the bottom of the screen, flip it and take `hud_bottom.png`. Every shot
  so far was top-aligned, so the bottom margin in the code is a **guess** that
  mirrors the measured 28px top margin. This settles it. Skip if the toggle
  doesn't exist — it is a bonus, not a gate.

- [ ] **1.4** Log out to character select, then **quit the client**.

---

## 5. Phase 2 — offline, read the result

- [ ] **5.1 Capture.**

```powershell
pwsh tools\sync-corpus.ps1 -Label c-after -Settings settings_Default
```

- [ ] **5.2** Hand back: the tab list from 0.5, the banner count from 0.4, and the
  screenshots. Everything else is read out of the captures:

| Question | Read from |
|---|---|
| Did the client re-create the orphan frames? | `core_char_93622368.dat`, ids 156/181/219/221 |
| Did the client keep the window mapping, and does it still list every tab? | `core_user_32945923.dat` |
| Did the client rewrite the mapping into a different shape? | same, vs `c-staged` |
| What offset did the drag write? | `core_char_93622368.dat` `shipuialignleftoffset` |
| Does the capacitor centre still equal `reference_w/2 + offset`? | `hud_moved.png` + the above |
| Did the left button column move with the HUD? | `hud_moved.png` vs `hud_battleship.png` |
| Is the bottom margin 28 or something else? | `hud_bottom.png` |

---

## 6. What this session does NOT cover

Deliberately, to keep it to one launch:

- **The dock/undock window diff** (open ledger entry). It needs geometry written
  at *two* different logout states, i.e. two full login cycles, because
  `windowSizesAndPositions_1` is only written on logout. Worth its own session
  and pairs naturally with the per-environment canvas view.
- **Transcribing the in-game keybinding screen** into `command-defaults.json`.
  A large manual transcription, not a check — it wants a dedicated sitting.
- **names-and-noise items 2-6.** Items 4-6 need no client at all, and 2-3 are
  editor-side judgements that can be made offline.
