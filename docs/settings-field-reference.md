# EVE settings field reference

A systematic inventory of **every field** in the character and account settings
files, derived from the corpus rather than from guesses, plus what it implies for
the editors we could build.

Companion to `format-notes.md`: that document describes the *wire format* and the
mappings we established experimentally; this one describes the *content* — the
whole key surface, what each key holds, how the keys relate, and which are
already covered by the app.

No character names, account names or real numeric IDs appear below. Every id in
an example is a synthetic placeholder (`<charID>`, `<itemID>`, `<channelGUID>`,
`<stationID>`, `<typeID>`, `<presetName>`). Counts and shapes are measured.

---

## 1. Method

Measured over the newest corpus snapshot (`2026-07-22T120910Z_states-after`):

| | files | of which in a live `settings_Default` dir |
|---|---|---|
| `core_char_<id>.dat` | 384 | 109 |
| `core_user_<id>.dat` | 175 | 54 |

A throwaway Rust tool (built against `blue-marshal`, kept out of the repo) decodes
every file, resolves all `Shared`/`Ref` indirection, unwraps the
`(FILETIME, value)` leaf wrappers, and aggregates per key-path: how many files
contain it, its value kinds, container lengths, dict key kinds, and sample
values. Dict keys are split into **settings keys** (present in ≥ 2 % of files —
these get their own entry) and **instance keys** (per-item/per-window/per-session
state — collapsed into a single `*` entry). Two passes: pass 1 learns the key
frequencies, pass 2 emits with the collapsing applied.

Caveats worth knowing when reading the numbers:

- The corpus is one player's machine. Presence counts show *how universal* a key
  is, not how popular a setting is. A key at 313/384 is "written by the client for
  essentially every character that has been played recently"; the missing ~70 are
  stale files last written years ago.
- The snapshot includes the profile's own `settings_Default - backup` /
  `- before X` directories, so it spans client generations from ~2020 to
  2026-07-22. That is a feature — it is how legacy keys were identified — but the
  "live" column is the one to trust for "does the current client still do this".
- Absence of a key almost never means "off". It means *EVE has never written it
  for this character*, and the client falls back to a built-in default. This is
  already the rule for `stateColors` and `shipuialignleftoffset`; it holds
  file-wide.

---

## 2. Scopes: three, not two

| Scope | File | Written when | Editing it affects |
|---|---|---|---|
| **Machine** | `core_public__.yaml`, `prefs.ini` | client shutdown | every account and character on this PC/install |
| **Account** | `core_user_<accountID>.dat` | logout | every character on that account |
| **Character** | `core_char_<charID>.dat` | logout | one character |

The app currently models only the bottom two. The machine scope is a real third
tier holding the graphics/resolution/master-audio settings and is not marshal at
all — it is YAML using the same `(FILETIME, value)` convention (see §6).

---

## 3. Reading conventions (and the traps)

1. **`(FILETIME, value)` leaf wrapper.** Most settings leaves are a 2-tuple whose
   first element is a `Long` holding a Windows FILETIME. Established in
   `format-notes.md`; confirmed universal here.

2. **The timestamp itself is frequently a `Ref` or `Shared`.** Identical
   timestamps are deduplicated across a file, so element 0 of the wrapper is often
   `Ref(slot)` pointing at a `Long` defined elsewhere — *not* a bare `Long`. Any
   "is this a timestamped leaf?" test that matches `Value::Long` directly will
   miss a large fraction of real leaves and report them as plain 2-tuples. Resolve
   through the shared table first. (This bit the first version of the inventory
   tool; every value shape in this document is post-resolution.)

3. **Dict keys are frequently `Ref`s too**, including *inside* tuple keys —
   `(Ref→b"overviewScroll2", 3)` is the normal on-disk form of the overview
   width key. Compare keys by resolved value, never by raw node. `format-notes.md`
   already records this for the account file's root `ui` section key; it is
   general.

4. **Dict keys are not always byte strings.** Observed key kinds across the
   corpus: `bytes`, `str`, `int`, `tuple` (of any of these, nested), `bool`, and —
   in the account `ui` section, in 140/175 files — a literal `None` key. Anything
   walking these dicts must survive all of them and round-trip them unchanged.

5. **Values are not type-stable across client generations.** The same key can be
   `Bool` in one file and `Int 0/1` in another (`hideCorpTicker`,
   `FMBQsearchTitles`, `alliance`, `public`, `contracts_search_expander_advanced`,
   `updateOnBossChange`, …), or `Bytes` vs `Str` (`market_searchText`,
   `charSheetSelectedPanel`), or `Int` vs `Long` (`lastSeenNotificationId`). A
   reader that hard-matches one variant silently projects nothing on older files.

6. **Empty sections are normal.** Several top-level sections are *always* an empty
   dict — they are namespaces the client reserves but this corpus never
   populates (§4.1, §5.1).

---

## 4. Character file — `core_char_<charID>.dat`

Root is a dict of **14 sections** (9–13 present per file).

### 4.1 Section map

| Section | files | Content |
|---|---|---|
| `windows` | 384/384 | Window geometry, per-window flag dicts, stacks, ship-HUD offset, colour theme |
| `ui` | 384/384 | 296 distinct keys (171 common) — the general per-character UI state bucket |
| `notifications` | 384/384 | Notification badge position + last-seen bookkeeping |
| `dockPanels` | 370/384 | Geometry of the 5+ dockable panels (map, Agency, skill planner, …) |
| `notepad` | 384/384 | `activeNote` only |
| `autorepeat` | 384/384 | Per-module auto-repeat, keyed by item id |
| `autoreload` | 384/384 | Per-module auto-reload, keyed by item id |
| `unseenInventoryItems` | 80/384 | Per-container "new item" sets |
| `seenInventoryItems` | 14/384 | Ditto, the seen half |
| `shiptheme` | 203/384 | **always empty** |
| `enableWindowBlur` | 203/384 | **always empty** |
| `generic` | 384/384 | **always empty** |
| `inbox` | 384/384 | **always empty** |
| `zaction` | 384/384 | **always empty** |

### 4.2 `windows` — 19 keys

The window-id-keyed dicts (`openWindows`, `minimizedWindows`, …) are covered in
`format-notes.md`. What that document does not list:

| Key | files | Shape | Meaning |
|---|---|---|---|
| `__version__` | 384/384 | `Int` = 1 | Schema version of the windows section |
| `__usercopy__` | 384/384 | `Bool` = true | Marks the section as user-copied |
| `__clear_stored_compact_window_settings_that_match_the_default__` | 98/384 | `Bool` = true | One-shot migration flag |
| `wndColorThemeID` | 286/384 | `Bytes` | **Window colour theme**, one of `UI/ColorThemes/{Carbon,Gallente,Photon,Plasma}` |
| `baseColorTemp` | 272/384 | `None` in every file | Custom theme base colour — never set in this corpus |
| `hiliteColorTemp` | 272/384 | `None` in every file | Custom theme highlight colour — ditto |
| `neocomLocationInfo_3` | 2/384 | `List[Bytes]` e.g. `[b"nearest", b"sovereignty"]` | Neocom location-readout content |
| `shipuialignleftoffset` | 315/384 | `Float` (2 files `Int`) | Ship HUD horizontal offset — already modelled |

Window-id-keyed dicts, for completeness: `windowSizesAndPositions_1` (384),
`openWindows` (384), `minimizedWindows` (384), `stacksWindows` (367),
`preferredIdxInStack3` (367), `pinnedWindows` (352),
`isLightBackgroundWindows` (343), `isOverlayedWindows` (330),
`lockedWindows` (330), `collapsedWindows` (312), `compactWindows` (184).

Window ids come in three flavours in the same dict: plain names (`overview`,
`overview_1`, `fitting`), numeric strings (stack containers), and stringified
Python tuples (`('corpassets', <itemID>)`, `('myPlaces', (<folderID>, None))`,
`('RolesSummary', 'Container Access (Based at)')`).

### 4.3 `notifications` — 4 keys

| Key | files | Shape |
|---|---|---|
| `notification_badge_offset` | 313/384 | `Tuple(Int, Int)` — badge screen position |
| `notificationSettingsRepositionCount` | 313/384 | `Int` |
| `lastSeenNotificationId` | 374/384 | `Int` or `Long` |
| `lastSeenNotificationTime` | 331/384 | `Long` |

> **`notification_badge_offset` is in `notifications`, not `ui`.** There is no
> `ui → notification_badge_offset` anywhere in the corpus. See §9.1 — the shipped
> HUD editor looks for it under `ui`.

### 4.4 `dockPanels` — 7 panels, 9 fields each

Keys: `primary_map_panel` (320/384), `solar_system_map_panel` (315),
`ActivityTracker` (313), `SkillPlanner` (265), `careerPortal` (178),
`ShipTree`, `PaintTool`.

Each is a plain dict (no timestamp wrapper on the panel itself) with exactly:

| Field | Type | Example |
|---|---|---|
| `align` | Int | `0`, `8` |
| `dblToggleFullScreenAlign` | Int | `0` |
| `positionX` / `positionY` | Float 0..1 | `0.5` |
| `widthProportion` / `heightProportion` | Float 0..1 | `0.8` |
| `widthProportion_docked` / `heightProportion_docked` | Float 0..1 | `0.5`, `1` |
| `pushedBy` | List | `[]` |

This is **layout data the layout canvas does not currently draw** — proportional
rather than absolute, but it is the same problem domain as window geometry.

### 4.5 `ui` — 296 keys, grouped

The section is a flat bag. Grouped by domain, with the count of files carrying the
group's typical key:

**Overview column widths & sort (per tab)** — the two-file overview link.
`SortHeadersSizes` (316) → dict keyed by tuple `(b"overviewScroll2", tabIdx)` →
`{COLUMNNAME: widthPx}`. `SortHeadersSettings2` (314) → same keying →
`Tuple(columnName, ascending: Bool)`, i.e. **the per-tab sort column**.
`SortHeadersSizes` also carries a non-tuple `overview` key on some files.

**HUD / screen furniture.** `fightersDetachedPosition` (319) `Tuple(Int, Int)`.

**Window chrome.** `windowTransparency` (200) `Float`,
`windowTransparencyLightMode` (176) `Float`, `neocomSizeLocked` (297) `Bool`,
`window_compact_mode_default` (8) `Bool`.
Note `windowTransparency` also exists in the *account* `ui` section (136/175) —
which one the client honours is untested.

**Info panels** (the left-hand info panel stack, per context).
`InfoPanelModes_hangar` (360), `_charsel` (352), `_inflight` (332),
`_structure` (325), `_skill_plan` (265), `_ActivityTracker` (313),
`_ActivityTracker_dockablePanel` (177), `_planet`, `_starmap_new`,
`_systemmap_new`, `_None`. Shape: `List[[panelID: Int, mode: Int]]` — an
ordered list of panel/mode pairs.

**Navigation & autopilot.** `autopilot_waypoints` (338) `List[<solarSystemID>]`,
`autopilot_avoidance2` (313) `List[<solarSystemID>]`, `pfRouteType` (322)
`Bytes` = `safe`/`shortest`, `pfAvoidSystems` (313) `Bool`, `pfPenalty` (272)
`Int`, `pfAvoidEdencomSystems` (176) `Bool`, `pathFinder_includeJumpGates` (272)
`Bool`.

**Drones.** `droneAggression` (313) `Bool`, `droneFocusFire` (313) `Bool`,
`drone_warp_warning_enabled` (273) `Bool`, `dronesViewMode` (178) `Int`.

**Ship behaviour defaults.** `defaultTypeOrbitDist` (278),
`defaultTypeKeepAtRangeDist` (277), `defaultTypeWarpToDist` (313) — each a dict
`{<typeID or 0>: distanceMetres}`, where key `0` is the global default.

**Scanner.** `directionalScannerMode` (313) `Int`,
`directionalScannerShowCone` (313) `Bool`, `directionalScanFilterPos` (313)
`Float`, `directionalScanPanelOpen` (315) `Bool`, `probeScanPanelOpen` (315)
`Bool`.

**Map.** `mapview2_colormode_primary` (313) `Int`,
`mapview2_recent_colormode_primary` (313) `List[Int]`,
`mapview2_showJumpBridges_primary` (313) `Int`,
`mapview2_systemmap_markers_primary` / `_solarsystem` (313/314) `List[Int]`,
`mapview2_autoFocusEnabled_solarsystem` (315) `Bool`,
`mapViewColorModeScrollPosition_primary` (313) `Float`,
`mapDirectionPanelEmbedded` / `mapProbePanelEmbedded` (313) `Bool`,
`solarSystemView_loaded_solarsystem` (315) `Int`, `mapView_searchString` (314)
`Str`.

**Camera.** `spaceCameraID` (332) `Bytes` = `shiporbit` /
`shiporbitabyssalspace` / `tactical`, `orbitCameraAutoTracking` (314) `Bool`,
`sensorSuiteEnabled` (332) `Bool`.

**Fleet.** `setFleetFormation` (280) `Int`, `setFleetFormationSize` (280) `Int`,
`setFleetFormationSpacing` (278) `Int`, `fleet_watchlistcolors` (177) dict
`{<charID>: colour}`, `fleetfinder_showGroupAndHighStandingsFleets` (292) `Int`,
`fleetAdvert_lastAdvert` (314) dict or `Instance(utillib.KeyVal)`,
`fleetAdvert_lastAdvertAdvancedOptions` (236) dict of 8 fields,
`fleetReconnect` (367) `None` or `Tuple(Long, Long)`.

**Combat log formatting.** `damageMessages_config` (314) and
`generalMessages_config` (314), both `Tuple(Int, Bytes align, Int, Int|Float)`.

**Market / contracts / assets / wallet** — search state, filter ids, sort keys,
cached ISK values: `market_searchText`, `market_value`, `market_last_update`,
`market_requires_update`, `marketGroupID_groupList`, `assets*`, `corpAssets*`,
`contracts_*`, `mycontracts_filter_owner`, `plex_value`, `walletWindowTab`,
`walletWindowHeight`, `walletWindowCollapsed`, `journalIncursionTab`.

**Panel selection / last-viewed state** — `charSheetSelectedPanel`,
`corpWindow*SelectedPanel`, `skillCatalogueCombo`, `SkillPlanTopLevelPanel`,
`SkillPlanBrowserToggleBtnID`, `fittingInvCombo`, `careerPortal_*`,
`contacts_lastselected`, `bookmarkFolderAndSubfolder`,
`bookmarkExpiryByFolder_<folderID>_<subfolderID>`.

**Chat.** `chatchannels` (367) `List[Tuple(kind, channelKey, label)]` — the
character's joined channels; `chat_OldChannelsMigrated` (367) `Bool`.

A chat window's id under `windows` is **`chatchannel_` + the tuple's FIRST
element** — confirmed in-game 2026-07-28, for a named standing channel and a
private conversation alike:

| `chatchannels` entry | window id |
|---|---|
| `(b"corp", "corp_98835672", "Corp")` | `chatchannel_corp` |
| `(b"alliance", "alliance_99010468", "Alliance")` | `chatchannel_alliance` |
| `("private_40fcd4de…", "private_40fcd4de…", "Private Chat (2)")` | `chatchannel_private_40fcd4de…` |

This was previously inferred rather than observed, and the worry was that a real
id might carry a kind segment the key does not — in which case every chat window
would silently keep a wrong derived name with no error anywhere. It does not: a
private conversation's key already carries its own `private_` prefix, so both
kinds derive identically. The third element is the channel's display name, which
`windowLabels.ts` does not yet use (see `docs/small-tasks.md`).

**Neocom.** `neocomButtonRawData` (370) `List[Instance]`,
`neocomButtonRawDataOriginal` (367) `Tuple[Instance]` — the neocom button set.
Format recorded in `format-notes.md` §"Neocom buttons"; already modelled
(reorder/remove/add/reset).

**Per-item / session state — do not model.** `listgroups` (348, 561 keys),
`viewedWrecks` (367), `neoblinkByID` (290), `ScrollColumnHeader_State` (234),
`agencyFiltersByContentGroupID`, `containerSortIconsBy_*`, `contentScroller_*`,
`achievements_recently_*`, `tracked_opportunities_*`, `expanded_job`,
`infoPanelExpandedMission`, `corporation_recruitment_*`.

### 4.6 `autorepeat` / `autoreload`

Both are dicts keyed by a `Long` **item id** (a specific fitted module), value
`Int`. `autorepeat` → `0` or `1000`; `autoreload` → `0` or `1`. `autorepeat`
reaches 3625 entries on one character. Character-specific and item-specific:
worth documenting, not worth an editor, and **not** worth copying between
characters (the item ids will not exist on the target).

---

## 5. Account file — `core_user_<accountID>.dat`

Root is a dict of **10 sections**, all present in every file.

### 5.1 Section map

| Section | Keys | Content |
|---|---|---|
| `ui` | 955 (549 common) | Graphics/camera, chat, market, inventory, fitting, scanner, `editHistory`, … |
| `tabgroups` | 338 | Selected tab + label per window stack |
| `overview` | 35 | The whole overview configuration |
| `audio` | 22 | Per-channel volumes and mutes |
| `suppress` | 18 | Dismissed "don't ask me again" dialogs |
| `defaultoverview` | 3 | Which bundled overview pack is installed |
| `cmd` | 1 (`customCmds`) | **Keybindings** |
| `windows` | 1 (`neocomWidth`) | Neocom width |
| `localization` | 0 | **always empty** |
| `notifications` | 0 | **always empty** |

### 5.2 `overview` — 35 keys

Already modelled by the app: `overviewProfilePresets`,
`overviewProfilePresets_notSaved`, `overviewColumns`, `overviewColumnOrder`,
`tabsettings_new` / `tabsettings`, `tabsByWindowInstanceID`, `stateColors`,
`backgroundStates2`, `backgroundOrder2`, `flagStates2`, `flagOrder2`,
`applyToStructures`, `applyToOtherObjects`, `useSmallColorTags`, `useSmallText`,
`overviewBroadcastsToTop`, `hideCorpTicker`.

Not modelled:

| Key | files | Shape | Meaning |
|---|---|---|---|
| `shipLabels` | 134/175 | `List[dict]` | **Overview label composer.** Each entry: `pre` / `post` (markup `Bytes`), `state` (`Int` 0/1 = enabled), `type` (`Bytes`: `ship type`, `alliance`, `corporation`, `pilot name`, `ship name`, or `None`). Order in the list is the render order. |
| `stateBlinks` | 134/175 | dict keyed `(surface, stateID)` → `Bool` | Which states blink. Surfaces `background` **and** `flag`. Exactly 8 entries in every file that has it. |
| `activeOverviewPreset` | 140/175 | `Bytes` or `Tuple(b"notSaved", name)` | The currently selected preset. **140/140 resolve to a real `overviewProfilePresets` key.** |
| `targetCrosshair` | 134/175 | `Bool` | |
| `showInTargetRange` | 134/175 | `Bool` | |
| `showCategoryInTargetRange_6` / `_11` / `_18` | 134/175 | `Bool` | Per-category (6/11/18 = EVE category ids) |
| `showBiggestDamageDealers` | 134/175 | `Bool` | |
| `showModuleHairlines` | 134/175 | `Bool` | |
| `viewTactical` | 137/175 | `Bool` | |
| `viewTactical_camTactical` | 131/175 | `Bool` | |
| `presetHistoryKeys` | 132/175 | dict keyed `(contentHash, Int)` | Imported-pack MRU (read-only) |
| `restoreData` | 132/175 | `{name, data, timestamp}` | Last imported pack verbatim (read-only) |
| `tabsettings2` | 75/175 | dict | **Dead key** — see below |
| `unfiltered` | 2/175 | | rare |
| `filterOut` | 1/175 | | rare |

**Three generations of tab settings.** `tabsettings` (138/175), `tabsettings2`
(75/175) and `tabsettings_new` (48/175) coexist and hold *different* content in
the same file. Resolved from the experiment diffs (`testdata/exp3a-user.diff`,
`exp3b-user.diff`): an in-game column add/reorder rewrites `tabsettings_new`
with the real change, timestamp-bumps `tabsettings` **without changing its
content**, and does not touch `tabsettings2` at all — not even its timestamp.
One account in the corpus (last written 2026-07-22, i.e. by the current client)
carries `tabsettings_new` **only**, with neither older key present.
So the ordering is: `tabsettings_new` authoritative, `tabsettings` a legacy
mirror kept alive for old clients, `tabsettings2` an abandoned intermediate
generation. The app's read order (`tabsettings_new` then `tabsettings`) is
correct; `tabsettings2` should be left alone, which it is.

**Confirmed against the whole corpus, 2026-07-28.** Scanning all **174 distinct
account files** (2,897 copies deduped by content), 130 carry at least one tab key:

| Keys present | Files |
|---|---|
| `tabsettings` + `tabsettings2` + `tabsettings_new` | 60 |
| `tabsettings` only | 46 |
| `tabsettings_new` only | 12 |
| `tabsettings` + `tabsettings2` | 11 |
| `tabsettings2` + `tabsettings_new` | 1 |
| **`tabsettings2` only** | **0** |

Two facts justify ignoring it:

1. **`tabsettings2` is never the only tab key** (0 of 130). The read order can
   therefore never leave the editor showing an empty tab list while
   `tabsettings2` holds one.
2. **On every file carrying `tabsettings_new`, `tabsettings2` is older** — no
   exceptions among the 61 such files. Where the modern key exists, the
   abandoned one is always behind it.

There *are* 11 files where `tabsettings2` carries the newest timestamp, which
looks alarming until you see that **all 11 carry no `tabsettings_new` at all**.
They are pre-Photon backups (`settings_Default - before photon mandatory`, plus
one `core_user_13036531 - old.dat`) in which `tabsettings2` was written roughly
0.08s *after* `tabsettings` — the same save, written last.

**The one honest caveat.** On those pre-Photon files the two keys hold genuinely
different content (`tabsettings2` is about twice the size, with different presets
and column lists), and the editor reads `tabsettings`. Which one that era's
client read is unknown and now untestable — no current client reads a pre-Photon
file. It is only reachable by opening a historical backup, and nothing the editor
writes there would be loaded by a live client anyway.

### 5.3 `cmd → customCmds` — the keybindings

175/175 files, 101 distinct command names, 0–96 entries per file.

Value is a **tuple of Windows virtual-key codes**, or `None` for "not bound":

```
CmdActivateHighPowerSlot1   -> (81,)          Q
CmdActivateMediumPowerSlot1 -> (17, 81)       Ctrl+Q
CmdDronesReturnAndOrbit     -> (18, 16, 68)   Alt+Shift+D
CmdActivateLowPowerSlot1    -> None           unbound
```

Modifiers observed: 16 = Shift, 17 = Ctrl, 18 = Alt; the remainder are standard
VK codes. Two things cross-validate that reading: the codes decode to physically
adjacent keys (`Q S D F G H` for module slots 1–6), and EVE's *factory* module
bindings are F1–F8 (VK 112–119), which appear nowhere in any file's bindings
(see below for what that absence means). Command names group cleanly:

- `CmdActivate{High,Medium,Low}PowerSlot1..8` — module activation (24)
- `CmdOverload{High,Medium,Low}Power{Rack,Slot1..8}` — overload (27)
- `CmdDrones*`, `CmdLaunchFavoriteDrones`, `CmdReconnectToDrones`,
  `CmdSelectAllFighters` — drones/fighters
- `CmdApproachItem`, `CmdKeepItemAtRange`, `CmdWarpToItem`, `CmdAlignToItem`,
  `CmdDockOrJumpOrActivateGate` — navigation
- `CmdLockTargetItem`, `CmdUnlockTargetItem`, `CmdSelect{Next,Prev}Target`,
  `CmdToggleShipSelection`, `CmdToggleLookAtItem` — targeting
- `CmdFleetBroadcast_*`, `CmdSendBroadcast_Target` — fleet broadcasts
- `Open*` / `Toggle*` (`OpenFitting`, `OpenAssets`, `OpenMail`,
  `OpenSkillsWindow`, `ToggleProbeScanner`, …) — window shortcuts
- `CmdRefreshDirectionalScan`, `CmdRefreshProbeScan`, `CmdToggleAutopilot`,
  `CmdToggleTacticalOverlay`, `CmdSetSearchBarFocus`, `CmdShowItemInfo`

**Corrected: EVE writes the whole command table for the client build, not a
diff of user edits.** The per-file command-name sets nest strictly by client
generation — 79 ⊂ 90 ⊂ 91 ⊂ 92 ⊂ 93 names, each step adding exactly one
command — which is what a client-owned table looks like as CCP adds commands
over time; under "only user edits" the player would have had to touch exactly
one additional command per client version, monotonically, never removing one.
The F1–F8 absence noted above corroborates it: the factory default does not
survive anywhere in the file once overwritten. `None` means **unbound**, not
"fall back to the default" — and an account that has never opened the in-game
keybinding screen has an **empty** table, not a default one (verified on a live
Tranquility account file). See
`docs/superpowers/specs/2026-07-26-keybindings-editor-design.md` §2.4–§2.5 for
the full evidence.

### 5.4 `audio` — 22 keys

Three families, over two different channel sets:

| Family | Channels | Type | Meaning |
|---|---|---|---|
| `inactiveSounds_<ch>` | `advancedSettings`, `aura`, `boosters`, `explosions`, `impacts`, `music`, `planets`, `shipsound`, `stationext`, `stationint`, `structures` (11) | `Int` 0/1 | Channel muted |
| `custom_<ch>` | `atmosphere`, `secondaryinterfaces`, `shipeffects`, `shipsounds`, `turrets` (5) | `Float` 0..1 | Custom mix level |
| `soundLevel_<ch>` | `advancedSettings` + the 5 `custom_*` names (6) | `Int` / `Float` | Channel volume |

22 keys total. Note the asymmetry: the 11 mute toggles have no matching
`soundLevel_` entry, and the `custom_*` channels carry both a level and a
`soundLevel_custom_*` duplicate of it.

Master volumes live in the *machine* file (§6), not here.

### 5.5 `suppress` — dismissed dialogs

18 keys, all `suppress.<DialogName>` → `Int` (values `1` or `6` — presumably a
bitmask of which button was chosen / how long to suppress).

Observed: `AskQuitGame` (158/175), `AskActivateTech3Ship`,
`AskUndockWithModulesLackingSkill`, `AttackGoodNPCAbort1`,
`ConConfirmCreateContract`, `ConNonEmptyContainer2`, `ConNonEmptyShip`,
`ConfirmJumpTo{Edencom,Triglavian,Invaded,Unsafe}SS`, `ConfirmOneWayItemMove`,
`ExternalLinkWarning`, `GateTollConfirmUnAligned`, `InsAskAcceptTerms`,
`Multiple Pilot Training`, `TradeShipWarning`, `WormholeJumpingFromHiSec`.

There is a *separate* `ui → suppress` key (150/175, `Int` 0/1) — a global
on/off, not the same thing.

### 5.6 `tabgroups` — the stack tab state

338 keys, in pairs: `<windowID>` → `Int` (selected tab index within the stack) and
`<windowID>_names` → `Str` (that tab's display label, e.g.
`"Character: Information"`).

The window ids are the **numeric stack-container ids** minted in the *character*
file's `windows → stacksWindows`. So stack membership and tab order are
character-scoped while the *selected* tab and its label are account-scoped — a
scope split the window-stacks editor does not currently touch.

### 5.7 `defaultoverview` — 3 keys

`defaultOverviewID` (83/175) `Bytes` — the id of the bundled overview pack the
account has installed; `overviewID` (78/175) `Bytes` or `None`;
`defaultOverviewInformedOfUpdate` (83/175) `Int`. This is the hook EVE's own
"default overview pack" feature uses, adjacent to our import/export packs.

### 5.8 `ui` — 955 keys, grouped

**Graphics & camera (account-scoped)** — 161/175 files, the cluster that makes
Camera Shake a reliable account-write trigger:
`cameraShakeEnabled`, `cameraBobbingEnabled`, `cameraDynamicMovement`,
`cameraInertia` (Float), `cameraSensitivity` (Float), `cameraInvertY`,
`cameraOffset`, `invertCameraZoom`, `advancedCamera`, `offsetUIwithCamera`,
`effectsEnabled`, `explosionEffectsEnabled`, `trailsEnabled`, `turretsEnabled`,
`missilesEnabled`, `droneModelsEnabled`, `gpuParticlesEnabled`,
`modelSkinsInSpaceEnabled`, `NCCgreenscreen`,
`UI_ASTEROID_{ATMOSPHERICS,CLOUDFIELD,FOG,GODRAYS,PARTICLES}`. All `Int` 0/1
except the two `Float`s.

`disabledGuids` (141/175) — dict of up to 159 `effects.<Name>` keys: individually
disabled visual effects.

`spaceMouseSpeedCoefficient` / `spaceMouseAccelerationCoefficient` (150/175)
`Float`.

**HUD (account half).** `shipuialigntop` (131), `detachFighterUI` (130),
`displayFighterUI` (131) — already modelled.

**Chat.** `chatfontsize_chatchannel_<channel>` (`Int`, ~12),
`chatinputsize_chatchannel_<channel>` (`Int`),
`chatchannel_<channel>_userlistwidth` (`Int`),
`chatCondensedUserList_<channel>` (`Bool`),
`chatWindowBlink_chatchannel_<channel>` (`Int`),
`chatchannel_local_mode` (`Int`), `timestampchat` (`Bool`),
`logmessageamount` (`Int`, 1000), `guestCondensedUserList` (`Bool`),
`chatPlayerChannelsJoined` (dict of `player_<channelGUID>`),
`player_<channelGUID>Password` — **a chat channel password in plaintext**. Never
include this in an export or a shared pack.

**Market.** `market_filter_{highsec,lowsec,zerosec,jumps,price,quantity}`
(`Bool`), `market_filters_{buy,sell}orderdev` (`Bool`), and for each a
`minEdit_` / `maxEdit_` `Int` bound; `market_ticker_enabled`,
`marketselectorwidth_region`, `pricehistorytype`, `multiSellDuration`,
`quickbar` + `quickbar_lastid`.

**Inventory.** Per-container families keyed by container name or
`<name>_<itemID>`: `containerViewMode_*` (`Bytes` = `icons`/`details`),
`containerSortIconsBy_*` (`Tuple(field, direction)`), `invFiltersExpanded_*`,
`invTreeExpanded_*`, `invTreeViewEntryToggle_*`, `invLastOpenContainerData_*`,
`invTreeViewWidth_*`. Mostly per-item state.

**Fitting.** `fittingLeftPanel`, `fittingPanelLeft3`, `fitting_browserBtnID`,
`fitting_filter_ship_{personal,corp,community}Fittings`, `defaultFittingPosition`,
`showEmptySlots`, `showhavecpuandpower`, `lockOverload`, `slotOrder`,
`linkedWeapons_groupsDict`, `fitting_hardwareSearchField`.

**Scanner.** `probeScannerFilters` (dict of named filters),
`activeProbeScannerFilter` (`Str`), `scannerShowAnomalies`, `scan_angleSlider`
(`Float`), `scanner_rangeEditMode`, `dir_scanrange`,
`probescanning.resultFilter.{filters,activeFilterSet,showingAnomalies}`,
`scanner_presetInUse` — **which is an overview preset name** (129/129 resolve).

**Custom probe formations.** `probescanning.customFormations` (114/175) is a dict
`{Int formationID: (name, List[((Float x, Float y, Float z), Float range)])}` — the
saved probe arrangements from the scanner's formation menu.
`probescanning.selectedFormationID` (114/175) `Int` is the id of the active one,
`0` in every file that has it.

- **Every formation holds exactly 8 probe entries** (123 formations, 984 entries,
  no other length).
- `range` is `74798935350.0` in all 984 entries — exactly 0.5 AU in metres. It is
  a per-probe setting, and **0.5 AU is the floor for combat scanner probes**
  (core probes reach 0.25 AU), which is why the corpus never varies: these are
  combat-probe formations at the tightest range those probes allow, not a
  constant written by the client. Confirmed in-game 2026-08-04 — a formation
  authored at 0.25 AU, launched with Sisters Combat probes, came back 0.5 AU on
  all 8 entries. Whether a stored range is applied at all on load is still
  untested; author one at 4 AU (clear of every floor) and read the scanner to
  settle it. The format records **no probe type**, so an editor cannot validate
  the floor, only warn about it.
- The coordinates are metre offsets from the formation centre, not absolute
  positions. `"close"` spans ~±22e9 m (~0.15 AU); `"on grid"` ~±10e6 m
  (~10 000 km).
- **A formation saved in-game does not always come back as it was authored, and
  what it loses depends on where the ship was.** Measured in-game 2026-08-04, one
  authored cube of ±2 309 401 m per axis (4000 km from centre), 8 Sisters Combat
  probes, launched and re-saved from three locations:

  | where | saved back |
  |---|---|
  | sun warp-in (~1e9 m) | ±2 309 401 m per axis — exact |
  | ~42 AU | half-extents 2 097 152 / 2 359 296 / 2 359 296 m — a **box**, not a cube |
  | ~90 AU | ±2 097 152 m per axis |

  Overview distances confirm the probes really sat where the saved file says:
  4 000 km on all eight at the sun, 3 941 km on all eight at 42 AU (the
  half-diagonal of that box). So the loss happens in space, between the client
  reading the file and writing it back — not in the file format, and not in
  anything an editor does.

  <a id="probe-precision-speculation"></a>
  > **SPECULATIVE — everything from here to the end of this bullet is one
  > hypothesis fitted to three saves, not a corpus measurement.** Treat the three
  > rows above as the only established facts. Nothing in the codebase should
  > depend on this being right.
  >
  > The numbers land on powers of two: 2 097 152 = 2^21, 2 359 296 = 9 × 2^18.
  > That fits probe positions round-tripping through `float32` **absolute**
  > coordinates, where the representable step at magnitude `c` is
  > `2^(floor(log2 |c|) - 23)` m. Authored 2 309 401 m then rounds to 4 steps of
  > 2^19 on one axis and 9 steps of 2^18 on another — which is what the 42 AU box
  > shows, and would mean each axis quantises independently by its own
  > coordinate's magnitude, so distortion is anisotropic and not predictable from
  > the radius alone.
  >
  > If that is what is happening, the grid coarsens with distance as below. The
  > radius is the **worst case** for any single axis; an axis whose coordinate is
  > smaller than `r` gets a finer step. "Smallest faithful offset" is 50 × step,
  > i.e. the offset below which the worst-case error exceeds 1%.
  >
  > | distance from the sun | grid step | worst error per axis | smallest faithful offset |
  > |---|---|---|---|
  > | < 0.007 AU | ≤ 128 m | ≤ 64 m | ≤ 6 km |
  > | 0.007 – 0.014 AU | 128 m | 64 m | 6 km |
  > | 0.014 – 0.029 AU | 256 m | 128 m | 13 km |
  > | 0.029 – 0.057 AU | 512 m | 256 m | 26 km |
  > | 0.057 – 0.115 AU | 1.0 km | 512 m | 51 km |
  > | 0.115 – 0.23 AU | 2.0 km | 1.0 km | 102 km |
  > | 0.23 – 0.46 AU | 4.1 km | 2.0 km | 205 km |
  > | 0.46 – 0.92 AU | 8.2 km | 4.1 km | 410 km |
  > | 0.92 – 1.84 AU | 16.4 km | 8.2 km | 819 km |
  > | 1.84 – 3.67 AU | 32.8 km | 16.4 km | 1 638 km |
  > | 3.67 – 7.35 AU | 65.5 km | 32.8 km | 3 277 km |
  > | 7.35 – 14.7 AU | 131 km | 65.5 km | 6 554 km |
  > | 14.7 – 29.4 AU | 262 km | 131 km | 13 107 km |
  > | 29.4 – 58.8 AU | 524 km | 262 km | 26 214 km |
  > | 58.8 – 118 AU | 1 049 km | 524 km | 52 429 km |
  > | 118 – 235 AU | 2 097 km | 1 049 km | 104 858 km |
  >
  > Three saves cannot distinguish this from any other rule that happens to round
  > 2 309 401 the same way, and no boundary in the table has been tested. To
  > falsify it cheaply: author offsets that straddle a predicted band edge and
  > save from a known distance — the step should double as the ship crosses it.

- **The stored origin is not the probe centroid.** That same 42 AU save carries Z
  as −2 621 440 / +2 097 152: symmetric ±2 359 296 about a centre sitting
  262 144 m off the origin the file counts from. A formation physically
  centred on the ship can still read lopsided, so nothing downstream may assume
  symmetry or normalise to it.
- Ids are **small and reused, not minted per formation**: `0` is `"close"` in all
  114 files, `1` is `"on grid"` in 5. `"close"`'s coordinates differ in the low
  bits between files (3 distinct signatures across the 114) — the same authored
  formation re-saved and rounded differently, not three formations. The
  location-dependent loss measured above is the likely cause, though this has not
  been tested at `"close"`'s scale.
- **Id `-4` is a scratch slot, and its name is `Bytes`, not `Str`.** 4 files carry
  `-4: (b"tempFormation", …)` alongside `0`, holding coordinates within rounding
  distance of `"close"` — the client's copy of the formation being edited. So the
  ids are signed, negative ids are not user formations, and anything reading the
  name must handle both `Bytes` and `Str` (§3 trap 5).

**Fleet.** `listenBroadcast_{HealArmor,HealShield,HealCapacitor,Target,HoldPosition,InPosition,NeedBackup}`
(`Int` 0/1), `fleet_broadcastcolor_<type>`, `fleetHistoryFilter`,
`fleetFinderBroadcastsVisible`, `fleetfinder_{scope,range,standing}Filter`,
`updateOnBossChange`, `hideInfo`, `public`, `publicgood`, `corp`, `alliance`.

**Structure browser.** `structurebrowser_{all,my}Structures_filterContController_{services,structureTypes}_{IsActive,filter_<tuple>}`.

**Misc UI.** `windowTransparency` (`Float`), `defaultDockingView`,
`defaultStructureView`, `stationsLobbyTabs`, `showSensorOverlay`,
`autopilot_stop_at_each_waypoint`, `showZoomBtns`, `showReadout`,
`targetOrigin` (`Tuple(Float, Float)`), `targetOriginLocked`, `mapscale`,
`alignHorizontally`, `damageMessagesShow{Ship,Ticker,Weapon}`,
`charsheet_showSkills`, `notepadscrollistwidth`, `evemail_leftContWidth`,
`columnWidths_3` / `scrollsortby_3` / `primaryColumn_3` /
`smartSortDirection_3` / `filteredColumnsByDefault_3` (generic scroll-list
column state, 1136 keys in the last one).

**Autofill.** `editHistory` (154/175) — already modelled; up to 204 widget-path
lists.

**Bookkeeping — leave alone.** `SeenPlex`, `SeenTokenStorage_SeenTokens`,
`SeenEventRewards_*`, `RewardsWindow_seen`, `DLI_claimedRewardsByIdx{,2,3}`,
`hasShownPointerToLoginRewardWnd`, `newFeaturesAlreadySeen`, `freeSkillPoints`,
`rewardWndTabSelected`, `accountreftype`.

**Anomalies to preserve.** A key that is literally `None` (140/175) and a tuple
key `(b"windowTransparency", (b"user", b"ui"), <Float>)` (135/175) both live in
this dict. They are not settings we should surface, but they must survive
round-trip untouched.

---

## 6. Machine scope — `core_public__.yaml` and `prefs.ini`

One of each per profile directory (8 profiles in the corpus). YAML, same
`(FILETIME, value)` convention rendered as a 2-element sequence.

`core_public__.yaml` sections:

- **`device`** — the graphics settings the app has no access to today:
  `DeviceSettings` (backbuffer size/format, present interval, windowed flag),
  `WindowMode`, `WindowedResolution`, `FixedWindow`, `FixedWindowSettings`,
  `FullScreenResolution`, `UIScaleWindowed` / `UIScaleFullscreen` (+ their
  `SetAutomatically` flags), `antiAliasing`, `aoQuality`, `shaderQuality`,
  `shadowQuality`, `textureQuality`, `charTextureQuality`, `lodQuality`,
  `postProcessingQuality`, `reflectionQuality`, `volumetricQuality`,
  `upscalingSetting`, `upscalingTechnique`, `fsrMode`, `frameGeneration`,
  `dofEnabled`, `brightness`, `charClothSimulation`, `fastCharacterCreation`,
  `resourceCacheEnabled`.
- **`audio`** — the masters: `audioEnabled`, `masterVolume`, `uiGain`,
  `worldVolume`, `eveampGain`, `evevoiceGain`, `useCombatMusic`,
  `suppressTurret`, `limitVoiceCount`, `useOldJukeboxOverride`.
- **`ui`** — `clientFontSize`, `CSS_AdAlreadyDisplayed_*`, migration flags.
- **`generic`** — `showintro2`.

`prefs.ini` is a flat INI: `clusterMode`, `clusterName`, `languageID`,
`eulaagreed`, `newbie`, `host`/`port`, `machoNet.acceptThreadCount`, decimal/digit
separators. **`languageID` is the client language** and is the only genuinely
interesting key.

`WindowedResolution` here is the counterpart of the `screenW`/`screenH` baked into
every window rect in the character file — which is what makes a
"rescale this layout to my current resolution" feature possible without asking
the user for their resolution.

---

## 7. How the pieces relate

```
MACHINE  core_public__.yaml
         device.WindowedResolution ─────────┐  (= the screenW/screenH baked
         audio.masterVolume                 │   into every window rect)
                                            │
ACCOUNT  core_user_<acct>.dat               │
         overview.overviewProfilePresets ◄──┼── named by:
           ▲  ▲  ▲                          │     overview.activeOverviewPreset
           │  │  └──────────────────────────┼──── ui.scanner_presetInUse
           │  └─ tabsettings_new[i].overview │
           └──── tabsettings_new[i].bracket  │
                                             │
         overview.tabsettings_new[i] ────────┼── positional link ──┐
         overview.tabsByWindowInstanceID ────┘                     │
         tabgroups[<stackID>]  (selected tab + label)              │
         audio / cmd.customCmds / suppress / ui.*                  │
                                                                   │
CHARACTER core_char_<char>.dat                                     │
         windows.windowSizesAndPositions_1[overview, overview_1…] ◄┘
         ui.SortHeadersSizes[(overviewScroll2, i)]      (widths, per tab i)
         ui.SortHeadersSettings2[(overviewScroll2, i)]  (sort, per tab i)
         windows.stacksWindows      (member → stack container id) ──┐
         windows.preferredIdxInStack3 (container → member tab order)│
                                        └── same container id ──────┘
                                            keys tabgroups above
```

Relationships that matter for editing:

1. **Overview tabs are a three-way join.** Tab *definition* is account-scoped
   (`tabsettings_new`, keyed by tab index); which overview *window* shows which
   tabs is account-scoped and **positional**
   (`tabsByWindowInstanceID[i]` ↔ window id `overview` / `overview_<i>`); the
   window's *geometry* and its *column widths and sort* are character-scoped.
   Already handled; recorded here because it is the reason the overview editor is
   two-file.

2. **Preset names are referenced from four places** and are the file's only
   string-keyed foreign key: each tab's `overview` and `bracket` fields, the
   container's `activeOverviewPreset`, and `ui → scanner_presetInUse`. Measured:
   tab `overview` 1080/1080 resolve, tab `bracket` 1039/1063 (the rest name
   built-in default presets), `activeOverviewPreset` 140/140,
   `scanner_presetInUse` 129/129. Renaming or deleting a preset must retarget all
   four — see §9.2.

3. **Window stacks span both files.** Membership and tab order live in the
   character file; the selected tab and its label live in the account file's
   `tabgroups`, keyed by the same minted container id. A stack created by our
   editor gets no `tabgroups` entry — apparently harmless (EVE defaults to tab 0),
   but a batch copy of a layout between characters *on different accounts* moves
   the stacks without their tab selection.

4. **The HUD is split across scopes**, as already documented: offsets in the
   character file, the on/off toggles and neocom width in the account file.

5. **Column vocabulary is shared**: `overviewColumns`/`overviewColumnOrder`
   (account defaults), `tabsettings_new[i].tabColumns`/`tabColumnOrder` (per-tab
   override) and `SortHeadersSizes[(overviewScroll2, i)]` (character widths) all
   key on the same uppercase column names (`ICON`, `DISTANCE`, `NAME`, `TYPE`,
   `ALLIANCE`, `CORPORATION`, `FACTION`, `MILITIA`, `SIZE`, `VELOCITY`,
   `RADIALVELOCITY`, `TRANSVERSALVELOCITY`, `ANGULARVELOCITY`, `TAG`).

---

## 8. Coverage today

| Domain | Where | App coverage |
|---|---|---|
| Window geometry + flags | char `windows` | **Full** (Layout canvas) |
| Window stacks | char `windows` | **Full** (create/unstack/reorder) |
| Ship HUD / fighter UI / neocom | char + account | **Full** (badge section fixed, §9.1) |
| Overview columns (visibility, order, width) | account + char | **Full** |
| Overview tabs (create/rename/delete/reorder/move, windows) | account | **Full** |
| Overview filter presets + groups | account | **Full** |
| Overview states / colours / tags | account | **Partial** — `background` surface only; no blinks |
| Overview appearance booleans | account | **Partial** — 6 of 13 |
| Overview import/export packs | account | **Full** |
| Autofill / remembered text | account `ui.editHistory` | **Full** |
| Batch copy | both | **Full** for the categories it defines |
| Everything else below | — | **Raw tree only** |

Untouched by any typed editor: keybindings, audio, graphics/camera, suppressed
dialogs, ship labels, dockable-panel geometry, window colour theme, window
transparency, chat settings, market filters, drone/navigation defaults, tab-group
labels, and the entire machine scope.

---

## 9. Findings that affect shipped code

### 9.1 The HUD badge offset read the wrong section (bug — FIXED)

**Fixed 2026-07-26 in `07ef6f0`, before v0.15.0 was published** (the release was
still a draft, so the tag was re-cut rather than patched). Kept here because the
*shape* of the mistake is the lesson: every unit fixture in `hud.rs` built the
same shape the `FIELDS` table declared, so all 20 tests passed on the broken
field. The fix added `crates/settings-model/tests/hud_corpus.rs`, which projects
real corpus files and fails when a character-scoped anchor reads nothing anywhere
— it reports `0/4215` against the old section. No hand-built fixture can catch
this class of bug. The original finding follows.

`crates/settings-model/src/hud.rs:76-79` declared:

```rust
Field { name: "badge_x", section: b"ui", key: b"notification_badge_offset", … }
```

but in all 384 character files the key lives at
`root → notifications → notification_badge_offset` (313/384) and
**`root → ui → notification_badge_offset` does not exist anywhere in the corpus**.
Consequence: the badge anchor always projects as its `"0"` default, and
`set_hud_value` would mint a fresh `ui` key that EVE ignores while the real
`notifications` entry keeps its old value. `format-notes.md` §"HUD anchors" has the
same wrong path, and its unit fixture builds the wrong shape, so the tests pass.
`fightersDetachedPosition` under `ui` is correct.

### 9.2 Preset rename/delete leaves two dangling references

`rename_preset` / `delete_preset`
(`crates/settings-model/src/overview_presets.rs:149,176`) retarget the preset
key, the `_notSaved` mirror and every tab — but not
`overview → activeOverviewPreset` (140/140 files reference a preset by name) nor
`ui → scanner_presetInUse` (129/129). After a rename, the account's active preset
and the probe-scanner filter point at a name that no longer exists.

### 9.3 `stateColors` has a second surface

`format-notes.md` states `b"background"` is the only surface observed. In this
snapshot `(flag, 48)` also appears (2/175 files). The states editor's behaviour is
still correct — it reads and rewrites `background` and passes other surfaces
through untouched — but the claim in the notes is wrong, and the *colortag*
colours are a real (if rare) editable surface. `stateBlinks` uses both surfaces in
134/175 files.

### 9.4 The `(FILETIME, value)` timestamp is often a `Ref`

Worth adding to `format-notes.md` §"Value-wrapper convention": any code that
identifies a timestamped leaf by matching `Value::Long` on element 0 without
resolving shared references will misclassify a large share of real leaves.

---

## 10. Candidate editors

Ranked by value ÷ effort. "Scope" is what one edit changes.

### Tier 1 — high value, self-contained

**1. Keybindings editor (SHIPPED)** — account scope, `cmd → customCmds`.

**Shipped on the `keybindings-editor` branch** — see `KeybindsView.svelte` and
`crates/settings-model/src/keybinds.rs`. No longer the top candidate; kept here
because the analysis is still useful background. The original finding follows.

The single biggest missing feature. 101 commands, values are VK-code tuples, one
flat dict, no cross-file link, no structural editing (the dict already exists in
175/175 files). Needs one hand-authored vocabulary — VK code → key label — the
same pattern already used for overview state ids; command name → friendly label
+ group turned out to be harvestable from the client's own localization data (84
of 101 resolve, the rest fall back to a de-camelcased name). Unlocks the thing
EVE has never shipped: copy one account's keybinds to every other account. Add a
`Keybinds` batch category and it composes with the existing Batch view for
free.

**2. Overview appearance completion** — account scope, `overview`.
Extend `OVERVIEW_BOOLS` from 6 to 13 (`targetCrosshair`, `showInTargetRange`,
`showCategoryInTargetRange_6/11/18`, `showBiggestDamageDealers`,
`showModuleHairlines`, `viewTactical`, `viewTactical_camTactical`), add
`stateBlinks` as a per-state blink toggle next to the existing colour control,
and let the `flag` surface be edited alongside `background`. All of it reuses
machinery that already exists. Half a day.

Extra reason to do it: **the pack importer is already ahead of the UI here.**
`overview_pack.rs` applies `stateBlinks` in full (both surfaces) and applies
`shipLabels` verbatim — settings the user can then see change but cannot edit.
It also caps `userSettings` at the same 6 booleans (`overview_pack.rs:306`), so a
pack exported from EVE carrying `targetCrosshair` or `showInTargetRange` is
silently dropped on import. Widening `OVERVIEW_BOOLS` closes the UI gap and the
import gap in one edit.

**3. Fix §9.1 and §9.2.** Not features, but both are small and both are in code
that is already merged.

**4. Suppressed-dialog manager** — account scope, `root → suppress`.
18 known keys, `Int` values, plus a "re-enable all" that clears the section.
Tiny, and it is a genuine EVE annoyance with no in-game UI.

### Tier 2 — high value, needs a new surface

**5. Overview ship labels** — account scope, `overview → shipLabels`.
The in-game editor for this is notoriously bad. A list of ordered segments with
`type` (a fixed 5-value enum), an enable flag and `pre`/`post` markup strings.
Natural home is a new tab in the existing Overview view, next to Columns /
Filters / Appearance. The markup is EVE's `<color=…>` / `<fontsize=…>` /
`<b>` dialect — a live preview is the whole value proposition. It is already part
of the pack vocabulary (`shipLabels` / `shipLabelOrder`), so import/export
already round-trips it; this just makes it editable.

**6. Graphics & camera** — *two* scopes at once.
Account-scoped camera/effects toggles in `core_user → ui` (25 keys, all `Int`
0/1 plus two `Float`s) and machine-scoped quality/resolution/upscaling in
`core_public__.yaml`. The account half is trivial and immediately useful ("copy
my graphics settings to my other accounts"). The machine half needs a YAML
reader — but `yaml-rust2` is already a dependency for overview packs, and the
value convention is identical. Doing the account half alone is a clean first
slice; the machine half introduces the third scope and should be its own
decision.

**7. Audio** — account `audio` (22 keys) + machine `core_public__.yaml → audio`
(10 keys). Same shape as graphics, same two-scope split, less demand. Bundle it
with graphics rather than shipping it alone.

### Tier 3 — extensions to what exists

**8. Per-tab overview sort column** — char `ui → SortHeadersSettings2`.
`(columnName, ascending)` per tab, keyed exactly like the widths the Columns tab
already edits. A dropdown and a direction toggle in a place that already has the
tab list. Very cheap.

**9. Window colour theme + transparency** — char `windows → wndColorThemeID`
(4 known values), `ui → windowTransparency`, `windowTransparencyLightMode`,
`neocomSizeLocked`. Natural additions to the Layout view's side panel. Note the
account file also has a `windowTransparency`; establish which wins before
exposing both.

**10. Dockable panels** — char `dockPanels`, 5–7 panels × 9 proportional fields.
The layout canvas already draws absolute rects; these are proportional, so they
need their own treatment, but they are the missing half of "my screen layout".

**11. Stack tab labels** — account `tabgroups`. Show the selected tab and its
label on each stack in the Layout view, and carry them along when a batch copy
moves stacks between characters on different accounts.

**12. New batch categories** — Keybinds, Audio, Graphics, Suppress, ShipLabels.
Each is a single dict or a small key set in one file; the batch machinery already
does the hard part.

### Explicitly not worth modelling

Per-item state keyed by item/type/station ids (`autorepeat`, `autoreload`,
`seenInventoryItems`, `unseenInventoryItems`, `listgroups`, `viewedWrecks`,
`invTreeViewEntryToggle_*`, `containerSortIconsBy_*`, `filteredColumnsByDefault_3`,
`bookmarkExpiryByFolder_*`); cached economy values (`assets_value`,
`market_value`, `plex_value`, `contract_value` and their `_last_update` /
`_requires_update` siblings); "have I seen this yet" bookkeeping
(`SeenPlex`, `DLI_claimedRewardsByIdx*`, `newFeaturesAlreadySeen`, …); session
state (`fleetReconnect`, `restoreData`, `presetHistoryKeys`); and the always-empty
sections. They belong in the raw tree and nowhere else.

**One privacy note:** `core_user → ui → player_<channelGUID>Password` stores chat
channel passwords in plaintext. Whatever export, pack or batch-copy feature we add
must exclude it by name, not just by section.
