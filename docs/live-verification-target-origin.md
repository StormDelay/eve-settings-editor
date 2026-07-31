# Capture: `targetOrigin` — what does it anchor, and in what units

One in-game capture, one character, one logout. It settles the last unmodelled
screen-furniture key: `targetOrigin` / `targetOriginLocked`, believed to be the
locked-target display's anchor. Method is §4 of the live verification plan — two
points, because one reading cannot separate an origin from a scale.

## What is being settled

`core_user_<id>.dat` → root **`ui`** section:

```
b"targetOrigin":       (ts, (Float, Float))   # 0..1, normalised against something
b"targetOriginLocked": (ts, Int 0/1)
```

**Read the section off an *inlined* dump.** A plain `bmdump dump` puts these keys
in a place that walks back to `windows` — the same `neocomWidth` really is in.
Only `dump-inline` resolves the sharing into the true tree, and there they sit
under `ui`, two sections apart from `neocomWidth`. This is the `badge_x` trap
(v0.15.0 declared `ui`, the key was under `notifications`) with the sections
swapped, and no hand-built fixture would catch it.

Nothing in the character file relates to targets — a byte scan of every
`core_char_*.dat` in the corpus finds `target` only inside overview group names,
and the 243-key depth-2 universe of a whole snapshot's character files holds no
candidate — so if the target list has a stored position, this pair is it. Present in ~13 % of account files in
the corpus, with exactly two distinct values across every snapshot, which is the
signature of "written when dragged", like the other three anchors.

Three questions, in order of how much they cost to answer:

1. **Does this pair belong to the target display at all?** Nothing proves it yet
   — the name is the only evidence. Toggling the lock answers it for free.
2. **What is x normalised against?** `y` is an exact pixel over the screen
   height in both known corpus values (`7/72` = 140/1440, `67/96` = 1005/1440).
   `x` is *not* an exact pixel over 2560: `0.8113057…×2560 = 2076.94`. Candidate
   denominators: 2560, 2523 (= 2560 − `neocomWidth` 37), 2512.
3. **Which point of the element does it name** — a corner, or the centre? The
   fighter panel taught this the hard way: an element's visible extent changes
   with its contents, and the anchor is not always inside the visible part.

## Rig

| | |
|---|---|
| Character | **B1 Holy Storm `96821229`** |
| Account file under test | `core_user_13375506.dat` (account B, `stormdelay7`) |
| Profile | `g_eve_shared_cache_sharedcache_tq_tranquility` |
| Client size | 2560×1440 (349 of Holy Storm's 381 geometry entries; the other 32 are legacy 1920×1080) |

`targetOrigin` is **account-scoped**, so log in Holy Storm *alone*. A second
character on account B would race the same file on logout; a character on
account A is harmless but adds churn to the diff.

## Before — recorded 2026-07-31, client closed

Snapshot `testdata/corpus/2026-07-31T000558Z_target-origin-before` (175 files,
`settings_Default` only), taken with `tools\sync-corpus.ps1`.

```
targetOrigin       = (0.8113057324840764, 0.6979166666666666)
targetOriginLocked = 1
neocomWidth        = 37
shipuialigntop     = True
```

At 2560×1440 that reads as right-hand side, roughly two thirds down. (The naive
"both axes are fractions of the whole screen" gives (2076.9, 1005.0); y is right,
x is not — see the solved convention below.) That is point 1: the "before"
screenshot is what pairs a picture with those numbers.

## In-game steps

- [ ] **1.** Log in Holy Storm only. Undock and lock **one** object (an asteroid
      in a belt is enough — the target list does not draw with nothing locked).
- [ ] **2.** Screenshot, native resolution, no scaling. This is point 1's picture
      and must match the numbers above.
- [ ] **3.** Lock **two more**. Screenshot again *without moving anything*. The
      two shots at one stored value are what say which way the list grows from
      its anchor — the same trick that separated the ship HUD's capacitor-wheel
      anchor from the element's own centre (battleship vs frigate racks).
- [ ] **4.** Unlock the position if it will not drag. Whatever control does that
      — right-click on the list, or the UI-lock toggle — **`targetOriginLocked`
      flipping 1 → 0 in the diff is the proof that this pair is the target
      display's.** If nothing flips it, the pair is something else and the rest
      of this capture is measuring the wrong thing.
- [ ] **5.** Drag the list **far horizontally** — into the left third of the
      screen, well clear of every edge. Horizontal distance is what separates the
      three candidate denominators (they differ by only ~8 px at the current x,
      which is inside eyeball error). **Never drop it in a corner: corners
      clamp,** and a clamped value is not the value you dragged to.
- [ ] **6.** Screenshot at the new position, still with three targets locked.
- [ ] **7.** Quit the client. EVE writes its settings on logout, not on change.

## Read back

```powershell
# from the main checkout — the corpus and the "before" snapshot live there,
# not in a worktree's own testdata/
cd D:\claude\eve-settings-editor
tools\capture-diff.ps1 -Label target-origin-after -Against target-origin-before
```

## Results — 2026-07-31, drag done

Snapshot `2026-07-31T010914Z_target-origin-after`, diff in
`testdata/dumps/2026-07-31T010914Z_target-origin-after__vs__2026-07-31T000558Z_target-origin-before`.

```
targetOrigin       (0.8113057324840764, 0.6979166666666666) → (0.5442122186495176, 0.5222222222222223)
targetOriginLocked 1 → 0        (same timestamp as targetOrigin — written as a pair)
```

**Q1 answered — the pair is the target display's.** Unlocking the list flipped
`targetOriginLocked`, and stripping timestamp-only churn out of the account-file
diff leaves *nothing else* but these two keys and an unrelated chat-tab change.
Holy Storm's character file moved only chat/window state — no target key, which
closes the question of whether any of this is per character. It is not.

**Q2 half-answered — y is a plain screen fraction.** All three known values are
exact integers over the full screen height: `7/72` = 140/1440, `67/96` =
1005/1440, `47/90` = 752/1440. Origin at the top.

**Q3 answered by the second session** (snapshot `2026-07-31T012708Z_target-origin-after2`):
a third position plus an orientation toggle.

```
targetOrigin        (0.5442122186495176, 0.5222222222222223) → (0.21342443729903537, 0.5333333333333333)
alignHorizontally   False → True      ← the vertical/horizontal target layout toggle, root `ui`
```

**`alignHorizontally` is the orientation flag** — `False` stacks the list
vertically, `True` lays it out in a row. It sits in the account file's root `ui`
section beside `targetOrigin`, and until this capture nothing had identified it;
`docs/settings-field-reference.md` files it under "Misc UI" with the map keys.

## The convention — solved 2026-07-31

Four screenshots, native 2560×1440, UI scale 1.0 both sessions (`core_public__.yaml`
shows only timestamps moving). Positions measured by bright-pixel clustering, and
anchored on the `NN km` label row, which is 28 px wide in every shot and therefore
repeatable to well under a pixel.

| shot | stored value | orientation | anchor slot's `km` label centre |
|---|---|---|---|
| `target.png` | (0.8113057…, 0.6979167…) | vertical, 2 targets | (2034.0, 955.5) |
| `vertical.png` | (0.5442122…, 0.5222222…) | vertical, 3 targets | (1369.0, 702.5) |
| `horizontal.png` | same value | horizontal, 3 targets | (1369.0, 702.5) |
| `horizontal2.png` | (0.2134244…, 0.5333333…) | horizontal, 3 targets | (657.0, 718.5) |

**y** is a plain screen fraction: `A_y = f_y × 1440`, exact in all four values
(140, 1005, 752, 768).

**x is normalised over the screen width to the right of the neocom**:

```
A_x = M + f_x × (screenW − M)          M = the neocom's drawn width
```

and the stored float is an *exact* rational `p / (screenW − M)` — machine-exact,
not merely close:

| value | exact form | implied M |
|---|---|---|
| 0.8113057324840764 | 2038/2512 | 48 |
| 0.5442122186495176 | 1354/2488 | **72** |
| 0.21342443729903537 | 531/2488 | **72** |
| 0.813713832738803 (other profile) | 2053/2523 | 37 |

72 is measured, not fitted: in `target.png` the neocom occludes the nebula through
x = 71 and the background resumes at x = 72. **Which means M is recoverable from
the stored value itself** — take the exact rational, and `M = screenW − denominator`.
That matters, because M is *not* `neocomWidth`: this account has held
`neocomWidth = 37` untouched since 2022 while its writes moved 48 → 72. (Whether
72 is a neocom the player resized, or a drawn width that is some other function of
that setting, is unresolved and does not block the arithmetic.)

**The anchor is the list's outer edge, and the list grows toward the screen
centre.** Solving the three positions for scale, origin and slot offset at once
(two right-anchored, one left-anchored) gives the offset from the anchor to the
first slot's centre as **55.5 px**, sign flipping with the side — exactly half the
110 px horizontal pitch, which nothing in the fit forced. Origin came out 70.5,
against the 72 measured off the neocom.

**Orientation does not move the anchor.** `vertical.png` and `horizontal.png` were
taken at one stored value, and the anchor slot lands on the *same pixel* in both
(label centre x 1369.0, ring band y 578..644). The list pivots around the anchor
slot; the others fan out from it.

### Element geometry, at UI scale 1.0

| | |
|---|---|
| Slot pitch, vertical | **181 px** |
| Slot pitch, horizontal | **110 px** |
| Ring bright extent | 79 px wide |
| Anchor → first ring centre | 141 px along the growth axis, 55.5 px inward across it |
| Anchor → first `km` label centre | 49.5 px up, 55.5 px inward |

So an N-target list occupies, right-anchored: x `[A_x − 111, A_x]`, y
`[A_y − 181N, A_y]` vertical; x `[A_x − 110N, A_x]`, y `[A_y − 181, A_y]`
horizontal. Left-anchored mirrors in x.

## Then

Four `Field` entries in `crates/settings-model/src/hud.rs`, all
`HudScope::Account` and all `section: b"ui"` — **not** `b"windows"`:
`targetOrigin` `elem: Some(0)`/`Some(1)` (`HudKind::Float`), `targetOriginLocked`
and `alignHorizontally`. Then `HUD_NOMINAL` gains the 110×181 slot, and
`layout.ts` the matched placement/inverse pair `layout.test.ts` round-trips.

**The x pair is not the usual `px ↔ fraction` conversion, and must not be written
as one.** The denominator is `screenW − M`, and M is only knowable from the value
being edited (recover the exact rational, `M = screenW − denominator`). Two
consequences:

- Prefer editing as a **delta**: `f_new = f_old + Δpx / denominator(f_old)`. That
  never needs M and is exact.
- A **minted** value — the key absent, or a Layout preset carrying one account's
  value onto another — has no denominator to recover and no way to learn the
  target account's M. Writing one is a guess; the honest options are to fall back
  to the corpus-typical M = 72 and say so in the UI, or to refuse to mint. Decide
  before this ships, because `batch.rs` will happily carry the field.

`targetOriginLocked` matters more than it looks: at `1` the list cannot be dragged
in-game, so a value the editor wrote is one the player cannot correct by hand.

Account scope means one drag moves it for **every character on the account**,
like `neocomWidth`. The UI must say so.
