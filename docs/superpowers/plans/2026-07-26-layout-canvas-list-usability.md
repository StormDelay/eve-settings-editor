# Layout canvas & list usability (slice 1a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Layout editor usable on a real character file — median 296 window rows and 68 overlapping canvas rectangles — by naming windows, folding repeated families, and filtering the list and the canvas together.

**Architecture:** Frontend only. A new pure module `windowLabels.ts` turns a raw window id into `{label, detail, family}`; `layout.ts` gains a pure filter predicate and `stackUnits` learns an optional visible-id set; `WindowPanel.svelte` renders friendly labels and folds families; `LayoutView.svelte` owns the filter state and feeds it to both the panel and the canvas. No Rust, no wire-format change — `WindowRect.label` keeps carrying the raw id.

**Tech Stack:** SvelteKit 2 + Svelte 5 runes, TypeScript 5.6, `node --test` (types stripped by Node, zero test dependencies).

**Spec:** `docs/superpowers/specs/2026-07-26-layout-canvas-list-usability-design.md`

## Global Constraints

- **Frontend only.** No changes under `crates/`. No new npm dependencies — the repo is deliberately dependency-light (`app/package.json` has three runtime deps, all Tauri).
- **Tests are throw-based**, no framework: a local `check(name, ok)` helper that throws on false, matching `layout.test.ts` and `search.test.ts`. Run with `npm test` from `app/`.
- **Folding is list presentation only; filtering is the shared mechanism.** Collapsing a family group must never remove a rectangle from the canvas.
- **A filtered stack keeps its unfiltered `anchor` and `fanTargets`.** The anchor is the geometry source and the drag target; `fanTargets` is what a drag writes to. Filtering either one would move the stack or strand its hidden members.
- **Nothing hides silently.** Whenever the filter is narrowing, the canvas shows `showing N of M windows` with a reset.
- **Dark native controls.** Any new `<input>`, `<select>` or `<option>` needs explicit `background: var(--bg)` and `color: var(--fg)` — native controls render light-on-white in this WebView2 shell otherwise. Available custom properties: `--bg`, `--bg-panel`, `--fg`, `--fg-dim`, `--accent`, `--danger`, `--ok`, `--warn`, `--border`.
- **Commit messages are sentence-case with no attribution trailers.**
- Verification for every task: `cd app && npm test && npm run check`.

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `app/src/lib/windowLabels.ts` | create | Pure: raw window id → friendly `{label, detail, family}`; the noise-family set; family grouping. No DOM, no Svelte, no api types. |
| `app/src/lib/windowLabels.test.ts` | create | `node --test` cases for every resolution rule. |
| `app/src/lib/layout.ts` | modify | Add the `WindowFilter` type and its pure predicate; teach `stackUnits` an optional visible-id set. |
| `app/src/lib/layout.test.ts` | modify | Cases for the predicate and for filtered `stackUnits`, including the no-regression default. |
| `app/src/lib/WindowPanel.svelte` | modify | Friendly labels, family folding, the filter controls, the context menu on rows and fields. |
| `app/src/lib/LayoutView.svelte` | modify | Owns filter state; feeds `stackUnits`; renders the `showing N of M · reset` counter. |
| `app/src/lib/ContextMenu.svelte` | create | Minimal positioned menu: `{x, y, items, onClose}`. |

---

### Task 1: The naming module

**Files:**
- Create: `app/src/lib/windowLabels.ts`
- Test: `app/src/lib/windowLabels.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `interface WindowName { label: string; detail: string; family: string }`
  - `function describe(id: string): WindowName`
  - `const NOISE_FAMILIES: ReadonlySet<string>`
  - `function groupByFamily<T extends { id: string }>(items: T[]): { family: string; label: string; items: T[] }[]`

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/windowLabels.test.ts`:

```ts
// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { describe, groupByFamily, NOISE_FAMILIES } from "./windowLabels.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

// --- rule 1: stringified Python tuple ids ----------------------------------
{
  const n = describe("('corpassets', 1037014587783L)");
  check("tuple id families on its first element", n.family === "corpassets");
  check("tuple id gets a curated label", n.label === "Corp assets");
  check("tuple id keeps the remainder as detail", n.detail === "1037014587783L");

  const nested = describe("('myPlaces', (12345, None))");
  check("nested tuple id still families on element 1", nested.family === "myPlaces");
  check("nested tuple id keeps the whole remainder", nested.detail === "(12345, None)");

  const unknown = describe("('RolesSummary', 'Container Access')");
  check("uncurated tuple id falls back to pretty()", unknown.label === "Roles Summary");
}

// --- rule 2: all-digit ids are minted stack containers ----------------------
{
  const n = describe("76");
  check("numeric id is a window stack", n.label === "Window stack");
  check("numeric id keeps the number as detail", n.detail === "76");
  check("numeric ids share one family", n.family === "stack");
}

// --- rule 3: parameterized families, longest prefix wins --------------------
{
  const chat = describe("chatchannel_local");
  check("chatchannel_ is a Chat", chat.label === "Chat");
  check("chat detail is the channel", chat.detail === "local");
  check("chat family is the prefix", chat.family === "chatchannel");

  const player = describe("chatchannel_player_-78564080");
  check("opaque suffix segments are dropped", player.detail === "player");

  const guid = describe("chatchannel_private_0ee11e4f970011ea8e789abe94f5b483");
  check("hex GUID segments are dropped too", guid.detail === "private");

  // Longest prefix wins: `mail` is curated, `mail_readingWnd` is a family.
  const mail = describe("mail_readingWnd_380729425");
  check("longest prefix wins over a shorter curated id", mail.label === "Mail message");
  check("an all-opaque suffix is kept verbatim", mail.detail === "380729425");
  check("mail message family is the long prefix", mail.family === "mail_readingWnd");
}

// --- rule 4: curated singletons --------------------------------------------
{
  check("curated exact id", describe("market").label === "Market");
  check("curated id has no detail", describe("market").detail === "");
  check("curated id families on itself", describe("market").family === "market");
  check("bare mail is still EVE Mail", describe("mail").label === "EVE Mail");
  check("a curated id that is also a prefix", describe("overview").label === "Overview");
  check("overview_1 goes through the family rule", describe("overview_1").detail === "1");
}

// --- rule 5: mechanical fallback -------------------------------------------
{
  check("boilerplate is stripped", describe("BugReportingWindow").label === "Bug Reporting");
  check("trailing New is boilerplate too", describe("AgencyWndNew").label === "Agency");
  check("camelCase is split", describe("multiFitWnd").label === "Multi Fit");
  // An all-lowercase run cannot be split — documented as accepted, not a bug.
  check("lowercase runs stay one word", describe("attributerespecification").label === "Attributerespecification");
}

// --- invariants ------------------------------------------------------------
{
  const ids = [
    "", "76", "market", "chatchannel_local", "('corpassets', 1L)",
    "Window", "___", "mail_readingWnd_1",
  ];
  for (const id of ids) {
    const n = describe(id);
    check(`describe(${JSON.stringify(id)}) has a non-empty label`, n.label.length > 0);
  }
}

// --- the noise set ---------------------------------------------------------
{
  check("chatchannel is noise", NOISE_FAMILIES.has("chatchannel"));
  check("mail_readingWnd is noise", NOISE_FAMILIES.has("mail_readingWnd"));
  check("market is not noise", !NOISE_FAMILIES.has("market"));
  // Every noise family must be a family describe() actually produces, or the
  // "hide chat & session windows" filter would silently match nothing.
  const samples: Record<string, string> = {
    chatchannel: "chatchannel_local",
    ChannelSettingsDlg: "ChannelSettingsDlg_fleet_1038711647935",
    ChatInvitation: "ChatInvitation_1111922349",
    mail_readingWnd: "mail_readingWnd_380729425",
    contactmanagement: "contactmanagement_98477766",
    groupInfoWnd: "groupInfoWnd_494332",
  };
  for (const fam of NOISE_FAMILIES) {
    check(`noise family ${fam} is reachable`, describe(samples[fam]).family === fam);
  }
}

// --- groupByFamily ---------------------------------------------------------
{
  const items = [
    { id: "market" },
    { id: "chatchannel_local" },
    { id: "overview" },
    { id: "chatchannel_corp" },
  ];
  const groups = groupByFamily(items);
  check("groups preserve first-seen order", groups.map((g) => g.family).join(",") === "market,chatchannel,overview");
  const chat = groups.find((g) => g.family === "chatchannel")!;
  check("a family collects all its members", chat.items.length === 2);
  check("a group is labelled by its family", chat.label === "Chat");
  check("singleton families are groups of one", groups[0].items.length === 1);
  check("empty input yields no groups", groupByFamily([]).length === 0);
}

console.log("windowLabels.test.ts ok");
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd app && npm test`
Expected: FAIL — `Cannot find module './windowLabels.ts'`.

- [ ] **Step 3: Write the implementation**

Create `app/src/lib/windowLabels.ts`:

```ts
// Friendly names for EVE window ids. Pure — no DOM, no Svelte, no api types —
// so it unit-tests under `node --test` like layout.ts and search.ts.
//
// A real character file carries a median 296 windows whose ids are raw client
// identifiers: `overview_1`, `ChannelSettingsDlg_fleet_1038711647935`,
// `('corpassets', 1037014587783L)`, `76`. This turns each into a readable
// label, an instance discriminator, and a grouping key.
//
// The tables are deliberately incomplete: an id nobody has curated falls
// through to `pretty()`, which is ugly but never wrong, and the raw id is
// always shown alongside. Grow CURATED/PARAM lazily as ids show up.

export interface WindowName {
  /** Friendly display name: "Chat", "Market", "Mail message". */
  label: string;
  /** The instance discriminator, shown dim beside the label; "" when singular. */
  detail: string;
  /** Grouping key — every id with the same family folds into one group. */
  family: string;
}

/**
 * Exact-id → label, for windows that exist once per character. Ids `pretty()`
 * already gets right (`AgencyWndNew`, `BugReportingWindow`, `multiFitWnd`, …)
 * are deliberately absent — curating them would be duplication, and the
 * fallback tests need real uncurated ids to be worth anything.
 */
const CURATED: Record<string, string> = {
  overview: "Overview",
  overviewsettings: "Overview Settings",
  market: "Market",
  marketbuyaction: "Market Order",
  MultiBuy: "Multibuy",
  fittingWnd: "Fitting",
  ViewFitting: "Fitting (View)",
  FittingMgmt: "Fitting Management",
  charactersheet: "Character Sheet",
  assets: "Assets",
  walletWindow: "Wallet",
  droneview: "Drones",
  selecteditemview: "Selected Item",
  watchlistpanel: "Watchlist",
  fleetwindow: "Fleet",
  FleetComposition: "Fleet Composition",
  RegisterFleetWindow: "Fleet Advert",
  mail: "EVE Mail",
  NewMessageWindow: "New Mail",
  notepad: "Notepad",
  mapbrowser: "Map Browser",
  MapCmdWindow: "Map",
  directionalScannerWindow: "Directional Scanner",
  probeScannerFilterEditor: "Scanner Filters",
  InventoryStation: "Inventory (Station)",
  InventorySpace: "Inventory (Space)",
  InventoryStructure: "Inventory (Structure)",
  corporation: "Corporation",
  addressbook: "People & Places",
  addressBookSearch: "People & Places Search",
  contracts: "Contracts",
  contractdetails: "Contract",
  createcontract: "Create Contract",
  redeem: "Redeem Queue",
  StructureBrowser: "Structure Browser",
  KillReportWnd: "Kill Report",
  infowindow: "Show Info",
  ChatWindowStack: "Chat stack",
  invitestack: "Invitation stack",
  XmppChatChannels: "Chat Channels",
  logger: "Combat Log",
  previewWnd: "Preview",
  typecompare: "Compare Tool",
  help: "Help",
  lobbyWnd: "Station Services",
  cloneBay: "Clone Bay",
  CloneUpgradeWindow: "Clone Upgrade",
  TransferMoney: "Give ISK",
  tradeWnd: "Trade",
  bookmarkLocationWindow: "Save Location",
  LinkedBookmarkFolderWindow: "Bookmark Folder",
  GroupsWnd: "Groups",
  EditMemberDialog: "Edit Member",
  broadcastsettings: "Broadcast Settings",
  NotificationSettings: "Notification Settings",
  PortraitWindow: "Portrait",
  ScreenshotEditingWnd: "Screenshot",
  InsuranceTermsWindow: "Insurance",
  corpassets: "Corp assets",
  myPlaces: "My Places",
};

/**
 * Prefix → label, for windows that exist once per channel / mail / contact.
 * The id is `<prefix>_<instance>`. Longest matching prefix wins, so adding a
 * shorter overlapping prefix later cannot steal a longer one's ids.
 */
const PARAM: Record<string, string> = {
  chatchannel: "Chat",
  ChannelSettingsDlg: "Chat settings",
  ChatInvitation: "Chat invitation",
  mail_readingWnd: "Mail message",
  contactmanagement: "Contacts",
  groupInfoWnd: "Info",
  ShipCargo: "Ship cargo",
  ShipDroneBay: "Drone bay",
  StructureShipHangar: "Ship hangar",
  containerWnd: "Container",
  containerContentWindow: "Container",
  overview: "Overview",
};

/**
 * The families that dominate a real file — 6 of the top 8 by row count. The
 * "hide chat & session windows" filter drops exactly these. Keys must be
 * families `describe` actually produces (asserted in the tests).
 */
export const NOISE_FAMILIES: ReadonlySet<string> = new Set([
  "chatchannel",
  "ChannelSettingsDlg",
  "ChatInvitation",
  "mail_readingWnd",
  "contactmanagement",
  "groupInfoWnd",
]);

/** Suffix segments that carry no meaning for a reader: ids, hashes, GUIDs. */
const OPAQUE = /^(-?\d+L?|[0-9a-f]{16,})$/i;

/** Client naming boilerplate that adds nothing to a label. */
const BOILERPLATE = /(Wnd|Window|Dlg|Panel|View|New)/g;

/** Mechanical fallback: strip boilerplate, split camelCase and _, title-case. */
function pretty(id: string): string {
  const words = id
    .replace(BOILERPLATE, " ")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[\s_]+/)
    .filter((w) => w.length > 0);
  // Everything was boilerplate or separators — keep the id rather than "".
  if (words.length === 0) return id;
  return words.map((w) => w[0].toUpperCase() + w.slice(1)).join(" ");
}

/**
 * The readable part of an instance suffix: leading segments up to the first
 * opaque one. `player_-78564080` → "player". When every segment is opaque
 * there is nothing to shorten, so the suffix is kept whole
 * (`380729425` stays `380729425`).
 */
function instanceDetail(rest: string): string {
  const kept: string[] = [];
  for (const seg of rest.split("_")) {
    if (OPAQUE.test(seg)) break;
    kept.push(seg);
  }
  return kept.length > 0 ? kept.join(" ") : rest;
}

const TUPLE_ID = /^\('([^']*)'\s*,?\s*/;

export function describe(id: string): WindowName {
  // 1. Stringified Python tuple: ('corpassets', 1037014587783L). Parsed
  //    shallowly on purpose — these ids are display material only, nothing
  //    writes them, so the remainder stays an opaque string.
  const tuple = TUPLE_ID.exec(id);
  if (tuple) {
    const family = tuple[1];
    const detail = id.slice(tuple[0].length).replace(/\)$/, "").trim();
    return { label: CURATED[family] ?? pretty(family), detail, family };
  }

  // 2. All digits: a stack container EVE minted. There is no name to find.
  if (/^\d+$/.test(id)) {
    return { label: "Window stack", detail: id, family: "stack" };
  }

  // 3. Parameterized family, longest prefix first.
  let best = "";
  for (const prefix of Object.keys(PARAM)) {
    if (prefix.length > best.length && id.startsWith(prefix + "_")) best = prefix;
  }
  if (best !== "") {
    return {
      label: PARAM[best],
      detail: instanceDetail(id.slice(best.length + 1)),
      family: best,
    };
  }

  // 4. Curated singleton, then 5. mechanical fallback. The `|| "(unnamed)"`
  //    guards the one input pretty() cannot name: the empty id.
  return { label: (CURATED[id] ?? pretty(id)) || "(unnamed)", detail: "", family: id };
}

/**
 * Bucket items by window family, preserving first-seen order so the list does
 * not reshuffle between renders. Generic over `{id}` to stay free of the api
 * types — callers pass `WindowRect[]`.
 */
export function groupByFamily<T extends { id: string }>(
  items: T[],
): { family: string; label: string; items: T[] }[] {
  const out: { family: string; label: string; items: T[] }[] = [];
  const byFamily = new Map<string, { family: string; label: string; items: T[] }>();
  for (const item of items) {
    const n = describe(item.id);
    let group = byFamily.get(n.family);
    if (!group) {
      group = { family: n.family, label: n.label, items: [] };
      byFamily.set(n.family, group);
      out.push(group);
    }
    group.items.push(item);
  }
  return out;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd app && npm test`
Expected: PASS — `windowLabels.test.ts ok`, and the existing suites still green.

Then: `cd app && npm run check`
Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/windowLabels.ts app/src/lib/windowLabels.test.ts
git commit -m "Name EVE window ids instead of showing them raw"
```

---

### Task 2: The shared filter predicate

**Files:**
- Modify: `app/src/lib/layout.ts` (add to the end; edit `stackUnits` at lines 75-104)
- Test: `app/src/lib/layout.test.ts` (append)

**Interfaces:**
- Consumes: `describe`, `NOISE_FAMILIES` from Task 1.
- Produces:
  - `interface WindowFilter { text: string; openOnly: boolean; hideNoise: boolean }`
  - `const NO_FILTER: WindowFilter`
  - `function filterIsActive(f: WindowFilter): boolean`
  - `function windowMatches(w: WindowRect, f: WindowFilter): boolean`
  - `function visibleIds(windows: WindowRect[], f: WindowFilter): Set<string>`
  - `stackUnits(layout: WindowLayout, visible?: Set<string> | null): DrawUnit[]` — the second parameter is new and optional; omitting it is exactly today's behaviour.

- [ ] **Step 1: Write the failing test**

Append to `app/src/lib/layout.test.ts`:

```ts
// --- the shared filter predicate -------------------------------------------
{
  const market = win("market", true, true);
  const chat = win("chatchannel_local", true, true);
  const closed = win("assets", false, true);

  check("an empty filter is not active", !filterIsActive(NO_FILTER));
  check("text makes it active", filterIsActive({ ...NO_FILTER, text: "a" }));
  check("whitespace-only text does not", !filterIsActive({ ...NO_FILTER, text: "  " }));
  check("openOnly makes it active", filterIsActive({ ...NO_FILTER, openOnly: true }));
  check("hideNoise makes it active", filterIsActive({ ...NO_FILTER, hideNoise: true }));

  check("an empty filter matches everything", windowMatches(chat, NO_FILTER));
  check("text matches the friendly label", windowMatches(market, { ...NO_FILTER, text: "mark" }));
  check("text matches case-insensitively", windowMatches(market, { ...NO_FILTER, text: "MARK" }));
  check("text matches the raw id", windowMatches(chat, { ...NO_FILTER, text: "chatchannel" }));
  check("text matches the detail", windowMatches(chat, { ...NO_FILTER, text: "local" }));
  check("text excludes a non-match", !windowMatches(market, { ...NO_FILTER, text: "zzz" }));
  check("openOnly drops a closed window", !windowMatches(closed, { ...NO_FILTER, openOnly: true }));
  check("openOnly keeps an open window", windowMatches(market, { ...NO_FILTER, openOnly: true }));
  check("hideNoise drops a chat window", !windowMatches(chat, { ...NO_FILTER, hideNoise: true }));
  check("hideNoise keeps a real window", windowMatches(market, { ...NO_FILTER, hideNoise: true }));

  const ids = visibleIds([market, chat, closed], { ...NO_FILTER, hideNoise: true, openOnly: true });
  check("visibleIds composes every clause", ids.size === 1 && ids.has("market"));
}

// --- stackUnits under a filter ---------------------------------------------
{
  const layout = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["m1", "m2"] }],
    windows: [
      win("C", true, true, { container_id: "C", role: "container" }),
      win("m1", true, true, { container_id: "C", role: "member" }),
      win("m2", true, true, { container_id: "C", role: "member" }),
      win("free", true, true, null),
    ],
  } as any;

  // No-regression: omitting the set is exactly today's behaviour.
  const before = stackUnits(layout);
  const same = stackUnits(layout, null);
  check("a null visible set is the unfiltered result", JSON.stringify(before.map((u) => u.key)) === JSON.stringify(same.map((u) => u.key)));

  // One matching member keeps the stack alive with only that tab.
  const oneTab = stackUnits(layout, new Set(["m1"]));
  const su = oneTab.find((u) => u.stack)!;
  check("a stack with one visible member survives", su !== undefined);
  check("only the visible member is a tab", su.tabs.map((t) => t.id).join(",") === "m1");
  check("the free window is filtered out", oneTab.length === 1);

  // The anchor and the fan are NOT filtered — the anchor is the geometry
  // source and the fan is what a drag writes to.
  check("the anchor ignores the filter", su.anchor.id === "C");
  const fan = su.fanTargets.map((w) => w.id).sort().join(",");
  check("fanTargets ignore the filter", fan === "C,m1,m2");

  // A stack with no visible member disappears entirely.
  const none = stackUnits(layout, new Set(["free"]));
  check("a stack with no visible member is dropped", none.length === 1 && none[0].key === "free");
  check("an empty visible set draws nothing", stackUnits(layout, new Set()).length === 0);
}
```

Add the new names to the import at the top of `layout.test.ts`:

```ts
import {
  canvasScale, toCanvas, toData, openWindows, resizeRect, stackUnits,
  NO_FILTER, filterIsActive, windowMatches, visibleIds,
} from "./layout.ts";
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd app && npm test`
Expected: FAIL — `NO_FILTER` is not exported from `./layout.ts`.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/layout.ts`, add the import at the top, beside the existing type import:

```ts
import { describe, NOISE_FAMILIES } from "./windowLabels";
```

Replace the body of `stackUnits` (currently lines 75-104) with the version below — only the signature, the `tabs` line and the free-window loop change:

```ts
/**
 * Group the open, renderable windows into draw units: one per stack (drawn at
 * the stack's anchor, with its open members as tabs in preferred order) and one
 * per non-stacked window. A stack with no open members — or whose anchor is not
 * open/renderable — is not drawn (nothing to show).
 *
 * `visible`, when given, is the set of window ids the filter admits. It narrows
 * which units are drawn and which tabs a stack shows, but NOT the unit's anchor
 * or its `fanTargets`: the anchor is where the rectangle's geometry comes from
 * and what a drag starts at, and `fanTargets` is what a drag writes to, so
 * filtering either would move the stack or strand its hidden members.
 */
export function stackUnits(layout: WindowLayout, visible: Set<string> | null = null): DrawUnit[] {
  const drawn = openWindows(layout.windows);
  const byId = new Map(drawn.map((w) => [w.id, w]));
  const renderableById = new Map(layout.windows.filter((w) => w.renderable).map((w) => [w.id, w]));
  const units: DrawUnit[] = [];
  const claimed = new Set<string>();
  const shown = (id: string) => visible === null || visible.has(id);

  for (const s of layout.stacks) {
    const tabs = s.members
      .map((id) => byId.get(id))
      .filter((w): w is WindowRect => !!w && shown(w.id));
    // A stack with no open members has nothing to show — hide it, and claim its
    // (possibly open) container so it doesn't fall through as a lone window.
    // Under a filter this also covers "no member matched".
    if (tabs.length === 0) {
      claimed.add(s.container_id);
      for (const id of s.members) claimed.add(id);
      continue;
    }
    const anchor = byId.get(s.anchor_id);
    if (!anchor) continue; // anchor not open/renderable — skip the stack
    // The container itself is not a tab unless it is also a member.
    for (const w of tabs) claimed.add(w.id);
    claimed.add(s.container_id);
    for (const id of s.members) claimed.add(id);
    const fanIds = new Set<string>([s.anchor_id, s.container_id, ...s.members]);
    const fanTargets = [...fanIds].map((id) => renderableById.get(id)).filter((w): w is WindowRect => !!w);
    units.push({ key: s.container_id, anchor, stack: s, tabs, fanTargets });
  }
  for (const w of drawn) {
    if (claimed.has(w.id) || !shown(w.id)) continue;
    units.push({ key: w.id, anchor: w, stack: null, tabs: [w], fanTargets: [w] });
  }
  return units;
}
```

> Note the added `for (const id of s.members) claimed.add(id)` in both branches: without it a stack member that the filter hid from the tab strip could fall through the free-window loop and draw a second rectangle on top of its own stack.

Then append to the end of `layout.ts`:

```ts
// --- filtering -------------------------------------------------------------
// One predicate, applied to the window list AND to what the canvas draws, so
// decluttering the list declutters the picture. Folding families in the panel
// is a separate, list-only affair — it must never reach this code.

export interface WindowFilter {
  /** Free text, matched against the friendly label, the detail and the raw id. */
  text: string;
  /** Drop windows EVE has not flagged open (roughly 77% of a real file). */
  openOnly: boolean;
  /** Drop the chat/mail/contact families that dominate a real file. */
  hideNoise: boolean;
}

export const NO_FILTER: WindowFilter = { text: "", openOnly: false, hideNoise: false };

/** Whether the filter narrows anything — drives the "showing N of M" line. */
export function filterIsActive(f: WindowFilter): boolean {
  return f.text.trim() !== "" || f.openOnly || f.hideNoise;
}

export function windowMatches(w: WindowRect, f: WindowFilter): boolean {
  if (f.openOnly && !w.open) return false;
  const n = describe(w.id);
  if (f.hideNoise && NOISE_FAMILIES.has(n.family)) return false;
  const q = f.text.trim().toLowerCase();
  if (q === "") return true;
  // Same contract search.ts documents for the tree: label, detail and the raw
  // id all match, so "market", "corpassets" and "1037014587783" all work.
  return `${n.label} ${n.detail} ${w.id}`.toLowerCase().includes(q);
}

export function visibleIds(windows: WindowRect[], f: WindowFilter): Set<string> {
  return new Set(windows.filter((w) => windowMatches(w, f)).map((w) => w.id));
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd app && npm test`
Expected: PASS, including the pre-existing `stackUnits` cases (the no-regression guard).

Then: `cd app && npm run check`
Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/layout.ts app/src/lib/layout.test.ts
git commit -m "Add one window filter the list and the canvas can share"
```

---

### Task 3: Friendly labels in the window list

**Files:**
- Modify: `app/src/lib/WindowPanel.svelte`

**Interfaces:**
- Consumes: `describe` from Task 1.
- Produces: no new exports. Visual change only.

- [ ] **Step 1: Import the namer and add a label snippet**

In the `<script lang="ts">` block of `WindowPanel.svelte`, add to the imports:

```ts
import { describe } from "$lib/windowLabels";
```

- [ ] **Step 2: Render the friendly name in `rowHead`**

Replace the `<button class="name">` in the `rowHead` snippet (the button rendering {w.label}; circa line 93 before this task's other edits):

```svelte
  {@const n = describe(w.id)}
  <button class="name" title={w.id} onclick={() => onSelect(w.id)}>
    {n.label}{#if n.detail}<span class="detail">{n.detail}</span>{/if}
  </button>
```

`{@const}` must be the first thing in the snippet body, so move it to the top of `rowHead`, above the existing `{@const openFlag = ...}`.

- [ ] **Step 3: Name the stack rows too**

Replace the `stack-title` span in the container-less stack head (the span rendering {stack.container_label}):

```svelte
          <span class="stack-title" title={stack.container_id}>{describe(stack.container_id).label}</span>
```

And in the two `<select>` dropdowns, replace `{s.container_label}` and `{other.label}`:

```svelte
                <option value={s.container_id}>{describe(s.container_id).label}</option>
```

```svelte
                <option value={other.id}>{describe(other.id).label}</option>
```

- [ ] **Step 4: Style the detail**

Add to the `<style>` block, after the `.name` rule:

```css
  .detail {
    color: var(--fg-dim);
    margin-left: 0.35rem;
    font-size: 0.9em;
  }
```

- [ ] **Step 5: Verify**

Run: `cd app && npm test && npm run check`
Expected: PASS, 0 errors. (No test asserts on Svelte markup — `windowLabels.test.ts` covers the naming; this step guards against a syntax or type error.)

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/WindowPanel.svelte
git commit -m "Show friendly window names in the layout list"
```

---

### Task 4: Fold repeated families in the list

**Files:**
- Modify: `app/src/lib/WindowPanel.svelte`

**Interfaces:**
- Consumes: `groupByFamily`, `describe` from Task 1.
- Produces: no new exports.

This is list presentation only — it must not touch `LayoutView`'s canvas.

- [ ] **Step 1: Group the free windows**

In `WindowPanel.svelte`, extend the import and replace the `freeWindows` derived (the one-line filter over windows with stack === null):

```ts
import { describe, groupByFamily } from "$lib/windowLabels";
```

```ts
  const freeWindows = $derived(windows.filter((w) => w.stack === null));
  // Folding is list presentation only: a family with more than one member
  // renders as one collapsible row. It never changes what the canvas draws —
  // that is the filter's job (LayoutView owns it).
  const freeGroups = $derived(groupByFamily(freeWindows));

  // Per-family collapse of the member rows. Families start folded: a real file
  // carries ~47 chat windows and folding is the whole point.
  let famOpen = $state<Record<string, boolean>>({});
```

- [ ] **Step 2: Render groups instead of a flat list**

Replace the whole `{#each freeWindows as w (w.id)}` block (the last top-level {#each} in the markup, over freeWindows) with:

```svelte
  {#each freeGroups as group (group.family)}
    {#if group.items.length === 1}
      {@render freeRow(group.items[0])}
    {:else}
      <div class="fam-group">
        <div class="fam-head">
          <button
            class="caret"
            aria-label="Expand family"
            aria-expanded={!!famOpen[group.family]}
            onclick={() => (famOpen[group.family] = !famOpen[group.family])}>
            {famOpen[group.family] ? "▾" : "▸"}
          </button>
          <span class="fam-title">{group.label}</span>
          <span class="stack-count">{group.items.length}</span>
        </div>
        {#if famOpen[group.family]}
          {#each group.items as w (w.id)}
            <div class="fam-member">{@render freeRow(w)}</div>
          {/each}
        {/if}
      </div>
    {/if}
  {/each}
```

- [ ] **Step 3: Extract the free row into a snippet**

Add this snippet next to the existing `rowHead` / `detail` snippets — it is the block that was just replaced, verbatim, wrapped as a snippet:

```svelte
{#snippet freeRow(w: WindowRect)}
  {@const stackTargets = freeWindows.filter((o) => o.id !== w.id && o.renderable)}
  <div class="row" class:selected={w.id === selectedId} use:scrollOnSelect={w.id === selectedId}>
    <div class="row-head">
      {@render rowHead(w)}
    </div>
    {#if w.renderable && (stacks.length > 0 || stackTargets.length > 0)}
      <div class="free-controls">
        {#if stacks.length > 0}
          <select
            aria-label="Add to stack"
            disabled={readOnly}
            value=""
            onchange={(e) => {
              const el = e.currentTarget as HTMLSelectElement;
              const v = el.value;
              el.value = "";
              if (v) onAddToStack(w.id, v);
            }}>
            <option value="" disabled>Add to stack…</option>
            {#each stacks as s (s.container_id)}
              <option value={s.container_id}>{describe(s.container_id).label}</option>
            {/each}
          </select>
        {/if}
        {#if stackTargets.length > 0}
          <select
            aria-label="Stack with another window"
            disabled={readOnly}
            value=""
            onchange={(e) => {
              const el = e.currentTarget as HTMLSelectElement;
              const v = el.value;
              el.value = "";
              if (v) onCreateStack(w.id, v);
            }}>
            <option value="" disabled>Stack with…</option>
            {#each stackTargets as other (other.id)}
              <option value={other.id}>{describe(other.id).label}</option>
            {/each}
          </select>
        {/if}
      </div>
    {/if}
    {#if w.id === selectedId && w.geom}
      {@render detail(w)}
    {/if}
  </div>
{/snippet}
```

- [ ] **Step 4: Auto-expand the family holding the selection**

A canvas click can select a window inside a folded family; the row must appear. Add after the `famOpen` declaration:

```ts
  // A canvas click can select a window inside a folded family — unfold it, or
  // the selection is invisible and scrollOnSelect has nothing to scroll to.
  $effect(() => {
    if (selectedId === null) return;
    const fam = describe(selectedId).family;
    if (freeGroups.some((g) => g.family === fam && g.items.length > 1)) {
      famOpen[fam] = true;
    }
  });
```

- [ ] **Step 5: Style the groups**

Add to the `<style>` block:

```css
  .fam-group {
    border-bottom: 1px solid var(--border);
  }
  .fam-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.5rem;
    background: rgba(255, 255, 255, 0.04);
    font-weight: 600;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .fam-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fam-member .row {
    border-bottom: none;
  }
  .fam-member .row-head {
    padding-left: 1.1rem;
  }
```

- [ ] **Step 6: Verify**

Run: `cd app && npm test && npm run check`
Expected: PASS, 0 errors.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/WindowPanel.svelte
git commit -m "Fold repeated window families in the layout list"
```

---

### Task 5: The shared filter, wired to both the list and the canvas

**Files:**
- Modify: `app/src/lib/LayoutView.svelte`
- Modify: `app/src/lib/WindowPanel.svelte`

**Interfaces:**
- Consumes: `WindowFilter`, `NO_FILTER`, `filterIsActive`, `visibleIds`, `stackUnits` from Task 2; `windowMatches` for the list.
- Produces: `WindowPanel` gains a `filter` prop (`$bindable`), so the panel renders the controls while `LayoutView` owns the state and applies it to the canvas.

- [ ] **Step 1: Own the filter state in `LayoutView`**

In `LayoutView.svelte`, extend the `layout.ts` import:

```ts
  import {
    canvasScale, toCanvas, toData, resizeRect, stackUnits, hudRects, shipOffsetFromX,
    hudPointFromRect, openWindows, NO_FILTER, filterIsActive, visibleIds,
    type Corner, type DrawUnit, type FurnitureRect, type WindowFilter,
  } from "$lib/layout";
```

Add the state beside `selectedFurniture` (after the selectedFurniture declaration):

```ts
  // The filter is shared: it narrows the window list AND what the canvas draws.
  // Folding families in the panel is separate and list-only.
  let filter = $state<WindowFilter>({ ...NO_FILTER });
```

Replace the `units` derived (the one calling stackUnits) and add the counter deriveds:

```ts
  const visible = $derived(
    layout && filterIsActive(filter) ? visibleIds(layout.windows, filter) : null,
  );
  const units = $derived(
    stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }, visible),
  );
  // Counted over drawable windows, not draw units — a stack is one unit but
  // several windows, and "showing 3 of 68 windows" is what the user is asking.
  const drawable = $derived(openWindows(layout?.windows ?? []));
  const shownCount = $derived(visible === null ? drawable.length : drawable.filter((w) => visible.has(w.id)).length);
```

- [ ] **Step 2: Reset the filter when the file changes**

A filter left over from another character is confusing. Leave `load()` alone; extend the existing reload `$effect` — the one guarding on `refreshToken !== lastToken || slot !== lastSlot || userOpen !== lastUserOpen` — so the filter clears only when the *slot* changes, not on every refresh:

```ts
  $effect(() => {
    if (refreshToken !== lastToken || slot !== lastSlot || userOpen !== lastUserOpen) {
      // A filter carried over from another character reads as "this file has
      // three windows". Clear it on a real file switch, but not on a save or
      // an account pairing, which must not disturb what the user is doing.
      if (slot !== lastSlot) filter = { ...NO_FILTER };
      lastToken = refreshToken;
      lastSlot = slot;
      lastUserOpen = userOpen;
      load();
    }
  });
```

- [ ] **Step 3: Render the counter**

Replace the `<p class="ref">` line (the one rendering `reference {layout.reference_w}x{layout.reference_h}`):

```svelte
      <p class="ref">
        reference {layout.reference_w}×{layout.reference_h}
        {#if filterIsActive(filter)}
          <span class="showing">
            · showing {shownCount} of {drawable.length} windows
            <button class="linkish" onclick={() => (filter = { ...NO_FILTER })}>reset</button>
          </span>
        {/if}
      </p>
```

Add to the `<style>` block:

```css
  .showing {
    color: var(--warn);
  }
  .linkish {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font: inherit;
    padding: 0;
    text-decoration: underline;
  }
```

- [ ] **Step 4: Pass the filter to the panel**

In the `<WindowPanel ... />` call, add:

```svelte
        bind:filter
```

- [ ] **Step 5: Accept and apply the filter in `WindowPanel`**

Add the import:

```ts
  import { windowMatches, NO_FILTER, type WindowFilter } from "$lib/layout";
```

In the `$props()` block, add the destructured entry as the **last** item of the
destructuring (after `onCreateStack,`):

```ts
    filter = $bindable({ ...NO_FILTER }),
```

and the matching entry as the last item of the type annotation (after
`onCreateStack: (m1: string, m2: string) => void;`):

```ts
    /** Shared with the canvas — see LayoutView. The panel renders the controls;
     * LayoutView owns the state and applies the same predicate to the rects. */
    filter?: WindowFilter;
```

Apply it to the list — replace the `freeWindows` derived from Task 4:

```ts
  const freeWindows = $derived(windows.filter((w) => w.stack === null && windowMatches(w, filter)));
```

and filter the stack member rows by wrapping the member `{#each}` body. In the stack section, change:

```svelte
        {#each stack.members as memberId, i (memberId)}
          {@const w = findWindow(memberId)}
          {#if w}
```

to:

```svelte
        {#each stack.members as memberId, i (memberId)}
          {@const w = findWindow(memberId)}
          {#if w && windowMatches(w, filter)}
```

- [ ] **Step 6: Render the filter controls at the top of the panel**

Insert as the first child of `<div class="window-panel">`:

```svelte
  <div class="filters">
    <input
      type="search"
      placeholder="Filter windows…"
      aria-label="Filter windows"
      bind:value={filter.text} />
    <label class="toggle">
      <input type="checkbox" bind:checked={filter.openOnly} />
      Open only
    </label>
    <label class="toggle">
      <input type="checkbox" bind:checked={filter.hideNoise} />
      Hide chat &amp; session windows
    </label>
  </div>
```

Add to the `<style>` block — the explicit colours are required, native controls render light-on-white in this shell otherwise:

```css
  .filters {
    display: grid;
    gap: 0.25rem;
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg-panel);
    z-index: 1;
  }
  .filters input[type="search"] {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 4px;
    font: inherit;
  }
  .filters input[type="search"]:focus {
    outline: 1px solid var(--accent);
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .toggle input {
    margin: 0;
  }
```

- [ ] **Step 7: Verify**

Run: `cd app && npm test && npm run check`
Expected: PASS, 0 errors.

- [ ] **Step 8: Commit**

```bash
git add app/src/lib/LayoutView.svelte app/src/lib/WindowPanel.svelte
git commit -m "Filter the window list and the canvas together"
```

---

### Task 6: The context menu

**Files:**
- Create: `app/src/lib/ContextMenu.svelte`
- Modify: `app/src/lib/WindowPanel.svelte`

**Interfaces:**
- Consumes: nothing from earlier tasks beyond `describe`.
- Produces: `ContextMenu.svelte` with a module-scope `export interface MenuItem { label: string; run: () => void }` and props `{ x: number; y: number; items: MenuItem[]; onClose: () => void }`.

This closes the `TODO(revisit)` at `WindowPanel.svelte:35-36` and the ledger's "panel right-click should be a context menu, not a direct tree jump".

- [ ] **Step 1: Create the component**

Create `app/src/lib/ContextMenu.svelte`:

```svelte
<script lang="ts" module>
  export interface MenuItem {
    label: string;
    run: () => void;
  }
</script>

<script lang="ts">
  // A flat right-click menu. Deliberately minimal: no submenus, no icons, no
  // portal — the panel is the only caller and it needs one list of actions.
  let {
    x,
    y,
    items,
    onClose,
  }: {
    x: number;
    y: number;
    items: MenuItem[];
    onClose: () => void;
  } = $props();

  function pick(item: MenuItem) {
    item.run();
    onClose();
  }
</script>

<svelte:window
  onpointerdown={onClose}
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }} />

<!-- stopPropagation so a click INSIDE the menu doesn't trip the window handler
     above and close it before the button's own onclick runs. -->
<div
  class="menu"
  role="menu"
  tabindex="-1"
  style="left: {x}px; top: {y}px;"
  onpointerdown={(e) => e.stopPropagation()}>
  {#each items as item (item.label)}
    <button role="menuitem" onclick={() => pick(item)}>{item.label}</button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 50;
    min-width: 11rem;
    padding: 0.2rem;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 4px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
  }
  .menu button {
    background: none;
    border: none;
    border-radius: 3px;
    color: var(--fg);
    cursor: pointer;
    font: inherit;
    padding: 0.25rem 0.5rem;
    text-align: left;
    white-space: nowrap;
  }
  .menu button:hover {
    background: var(--accent);
    color: var(--bg);
  }
</style>
```

- [ ] **Step 2: Replace the direct tree jump in `WindowPanel`**

Add the import:

```ts
  import ContextMenu, { type MenuItem } from "$lib/ContextMenu.svelte";
```

Replace the `reveal` helper and the `TODO(revisit)` comment above it — the block that starts `// Right-click a property to reveal` and ends with the `reveal` arrow function — with:

```ts
  // Right-click opens a menu. This replaces the M2-era direct tree jump — the
  // TODO that shipped with the layout canvas.
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function openMenu(e: MouseEvent, items: MenuItem[]) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, items };
  }

  const copyId = (id: string): MenuItem => ({
    label: "Copy window id",
    // Best-effort: a clipboard refusal must not throw into the click handler.
    run: () => void navigator.clipboard.writeText(id).catch(() => {}),
  });

  const showInTree = (path: NodePath): MenuItem => ({
    label: "Show in tree",
    run: () => onReveal(path),
  });

  // The item lists are built here, not inline in the template: `f.set` is a
  // discriminated union, and TypeScript only narrows `f.set.path` inside a
  // plain function body — a narrowing written into a template ternary does not
  // reach the arrow function it creates.
  function rowMenu(w: WindowRect): MenuItem[] {
    const items: MenuItem[] = [];
    if (w.geom) {
      const path = w.geom.x_path;
      items.push({ label: "Show geometry in tree", run: () => onReveal(path) });
    }
    items.push(copyId(w.id), { label: "Select on canvas", run: () => onSelect(w.id) });
    return items;
  }

  function flagMenu(w: WindowRect, f: BoolFlag): MenuItem[] {
    const items: MenuItem[] = [];
    if (f.set.how === "set") items.push(showInTree(f.set.path));
    items.push(copyId(w.id));
    return items;
  }
```

- [ ] **Step 3: Point the row and the fields at it**

In the `rowHead` snippet, add `oncontextmenu` to the name button:

```svelte
  <button
    class="name"
    title={w.id}
    onclick={() => onSelect(w.id)}
    oncontextmenu={(e) => openMenu(e, rowMenu(w))}>
    {n.label}{#if n.detail}<span class="detail">{n.detail}</span>{/if}
  </button>
```

In the `detail` snippet, replace the coordinate label's handler:

```svelte
        <label title="right-click for actions" oncontextmenu={(e) => openMenu(e, [showInTree(geomPath(w, field)), copyId(w.id)])}>
```

and the flag label's:

```svelte
        <label
          class="flag"
          title={f.set.how === "unavailable" ? "Not present in this file" : "right-click for actions"}
          oncontextmenu={(e) => openMenu(e, flagMenu(w, f))}>
```

- [ ] **Step 4: Render the menu**

Add as the last child of `<div class="window-panel">`:

```svelte
  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
  {/if}
```

- [ ] **Step 5: Verify**

Run: `cd app && npm test && npm run check`
Expected: PASS, 0 errors.

Note that the flag label now opens a menu unconditionally, where the old code left `oncontextmenu` undefined for an unavailable flag. That is intentional: *Copy window id* is still useful there, and `flagMenu` omits *Show in tree* when there is no path.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/ContextMenu.svelte app/src/lib/WindowPanel.svelte
git commit -m "Open a context menu on right-click instead of jumping to the tree"
```

---

### Task 7: Live smoke and ledger

**Files:**
- Modify: `docs/small-tasks.md`

Every slice ends with a live smoke against a real client. No code changes unless it surfaces a defect.

- [ ] **Step 1: Build and run**

```bash
cd app && npm run tauri dev
```

- [ ] **Step 2: Work the checklist on a real character with ~300 windows**

- [ ] The list is navigable: chat, mail, contacts and info windows are folded into counted groups, not ~200 loose rows.
- [ ] Labels read as EVE names, not raw ids; hovering a row shows the raw id.
- [ ] Typing in the filter box narrows the list **and** the canvas together, and the `showing N of M windows` count is correct.
- [ ] `Hide chat & session windows` visibly thins the canvas in one click.
- [ ] `reset` restores every window to both the list and the canvas.
- [ ] Clicking a rectangle on the canvas selects its row, unfolding its family if it was folded.
- [ ] **Dragging a window while a filter is active still writes the right geometry** — drag a stack in particular, then clear the filter and confirm every member moved together (the `fanTargets` invariant).
- [ ] Right-click a row, a coordinate and a flag: the menu opens at the pointer, *Show in tree* lands on the right node, *Copy window id* puts the raw id on the clipboard, Escape and an outside click both close it.
- [ ] Switching to another character clears the filter.

- [ ] **Step 3: Record what the smoke found**

Add any non-blocking minors to the **Open** section of `docs/small-tasks.md`, plus this item, which the spec defers deliberately:

```markdown
- [ ] **Stack labels from the account file's `tabgroups`.** The account file
  stores `tabgroups → <containerId>_names` → EVE's own tab label (e.g.
  `"Character: Information"`), which beats anything we derive from the container
  id. Slice 1a labels a stack from its anchor window instead, because taking the
  real string needs `window_layout` to accept the user root, which ripples
  through `ops.rs` and every call site. Supersedes half of the older
  "friendlier stack-frame labels" item. _Added 2026-07-26 (slice 1a design)._
```

- [ ] **Step 4: Commit**

```bash
git add docs/small-tasks.md
git commit -m "Ledger the slice 1a follow-ups"
```

---

## Done when

- `cd app && npm test` passes, including the new `windowLabels.test.ts` and the extended `layout.test.ts`.
- `cd app && npm run check` reports 0 errors.
- `cargo test --workspace` still passes (nothing under `crates/` was touched, so this is a sanity check, not a gate).
- The live smoke checklist in Task 7 is worked through on a real character file.
