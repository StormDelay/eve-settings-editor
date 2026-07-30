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
  assembleWindow: "Assemble",
  bookmarkLocationWindow: "Save Location",
};

// --- clutter -----------------------------------------------------------
// EVE spawns some windows per conversation, per item or per dialog — the
// player never placed them, and once opened they never really close (they
// pile up as ~160 closed rows in a real file). Others are windows the
// player actually positioned — even a chat, if it's Local/Corp/Alliance/
// Fleet rather than a one-off invitation or private convo. "Clutter" means
// the former only, regardless of open/closed: hiding it must be safe in
// both the list AND the canvas.
//
// There is no reliable "is this actually on screen" signal to check against:
// `openWindows` only accumulates the ids EVE would restore, it is never
// cleared, and nothing else stored (minimized/collapsed/geometry) tracks
// real visibility either. So this is a curated approximation, not a
// derivation — it will always be incomplete, and an unrecognised id is
// deliberately left visible rather than guessed at (the safe failure
// direction: showing a harmless extra row beats hiding a real window).
//
// `describe()` only assigns these families to a SUFFIXED id
// (`<family>_<instance>`); a bare id falls through to describe's rule 4/5
// and gets `family === id`, which for e.g. `ShipCargo` collides with the
// suffixed family string. So membership here is necessary but not
// sufficient — see isClutter, which additionally requires a non-empty
// `detail` to tell a spawned instance from its bare parent.

/** Families that exist ONLY as spawned instances — a parent window search
 * (`describe(id).family`) is not enough; isClutter also checks `detail`.
 * Every entry here must also be a PARAM prefix, or describe() never groups
 * the suffixed id into this family in the first place. */
const CLUTTER_FAMILIES: ReadonlySet<string> = new Set([
  "ChatInvitation",
  "ChannelSettingsDlg",
  "mail_readingWnd",
  "groupInfoWnd",
  "contactmanagement",
  "ShipCargo",
  "ShipDroneBay",
  "containerWnd",
  "StructureShipHangar",
  "assembleWindow",
  "bookmarkLocationWindow",
]);

/** Chat is clutter only for private/direct conversations — defined
 * positively so an unrecognised future channel is kept, not hidden. Standing
 * channels (local/corp/alliance/fleet/incursion/invasion) stay visible. */
const CLUTTER_CHAT_DETAILS: ReadonlySet<string> = new Set(["player", "private"]);

/** One-off transient dialogs: exact id, never a family (there's only ever
 * one at a time, so there's no parent/spawned distinction to make).
 * `bookmarkLocationWindow` is also in CLUTTER_FAMILIES — a real file carries
 * both the bare id and a suffixed `bookmarkLocationWindow_<itemID>`, and both
 * are transient; the exact-id check runs first so this needs no special case. */
const CLUTTER_IDS: ReadonlySet<string> = new Set([
  "setQuantityPopup",
  "setNewName",
  "mySearch",
  "DisconnectNotice",
  "NewFeatureNotifyWnd",
  "ScreenshotEditingWnd",
  "BugReportingWindow",
  "contractSelectItemTypeDlg",
  "addressBookSearch",
  "contractFinishStepSearch",
  "contractEndpointSearch",
  "ship_name_dialog",
  "enterShipPassword",
  "AddToBlockSearch",
  "kickCharacterFromChat",
  "skill_requirement_dialog",
  "message",
  "missingSkillbooksWnd",
  "locationsearch",
  "newMessageReceiverSearch",
  "AccessGroupsAddMember",
  "SellItemsWindow",
  "CrateWindow",
  "marketmodifyaction",
  "marketbuyaction",
  "createcontract",
  "contractdetails",
  "TaskConversationWindow",
  "WarReportWnd",
  "StoreFleetSetupWnd",
  "StoredFleetSetupListWnd",
  "bookmarkLocationWindow",
  "previewWnd",
  "tradeWnd",
  "MultiBuy",
  "overviewsettings",
  "TransferMoney",
  "EditMemberDialog",
  "InsuranceTermsWindow",
  "ActivateMultiTrainingWindow",
  "CloneUpgradeWindow",
  "multiFitWnd",
  "attributerespecification",
  "EngineTools",
  "outstandingcalls",
  "mapspalette",
  "CtrlTabWindow",
  "probeScannerFilterEditor",
  "GroupsWnd",
  "broadcastsettings",
]);

/** Per-window user overrides of the built-in clutter tables. The two sets are
 * kept disjoint by the UI; `visible` wins if a hand-edited file lists an id in
 * both. */
export interface ClutterOverrides {
  clutter: ReadonlySet<string>;
  visible: ReadonlySet<string>;
}

/** True for a window EVE spawns per conversation/item/dialog rather than one
 * the player placed. Hidden in both the list and the canvas, whether open or
 * closed — open/closed is not the axis; kind of window is.
 *
 * The built-in tables can never be complete (see the note above CLUTTER_IDS),
 * so a user override outranks them in both directions. */
export function isClutter(id: string, o?: ClutterOverrides): boolean {
  if (o?.visible.has(id)) return false;
  if (o?.clutter.has(id)) return true;
  if (CLUTTER_IDS.has(id)) return true;
  const n = describe(id);
  if (n.family === "chatchannel") return CLUTTER_CHAT_DETAILS.has(n.detail);
  // detail === "" means a bare parent window (e.g. plain "ShipCargo") — keep it.
  return CLUTTER_FAMILIES.has(n.family) && n.detail !== "";
}

// --- environments ----------------------------------------------------------
// A player's screen differs by environment, and the canvas mixes every
// environment into one picture. This is a VIEW FILTER, not a data model:
// `windowSizesAndPositions_1` stores one geometry per window id, so there is a
// single layout underneath and these sets only decide what is painted.
//
// Two environments, not EVE's thirteen. `ui → InfoPanelModes_<context>`
// enumerates the client's own list (hangar, inflight, structure, charsel,
// planet, starmap…), but only hangar/inflight/structure have an arrangeable
// window layout, and NPC station and player structure are collapsed into one
// "docked" view — which is also the split `dockPanels` itself stores
// (widthProportion_docked). See the design spec for the corpus measurements.
//
// Only the EXCLUSIVES are listed. An id in neither set shows in both views —
// the same safe-failure direction as the clutter tables: showing a harmless
// extra rectangle beats hiding a window the player actually placed. Windows
// whose environment is genuinely uncertain (Fitting, Assets, Market, the chat
// stack) are deliberately absent rather than guessed at. Grow these lazily.
//
// The two sets are not the same kind of evidence. `DOCKED_ONLY` corroborates
// against the corpus measurement (see the design spec's §2.2 — the Structure*
// hangar ids and the station windows showed up as genuinely docked-only in
// the file data). `SPACE_ONLY` is NOT: the corpus pass only observed
// docked-side exclusives, so every `SPACE_ONLY` entry is game-knowledge
// curation, not something measured. Do not mistake one for the other.

export type Env = "all" | "docked" | "space";

/** Windows that only exist while docked, in an NPC station or a player
 * structure. The Structure* ids have no station twin — the station equivalent
 * is the unified `InventoryStation`. `StructureCorpHangar` is rare but real —
 * 31 character files in the corpus carry it, against 3,997 for
 * `StructureItemHangar` — so it stays despite the low count.
 * `CloneUpgradeWindow` also appears in `CLUTTER_IDS`; that is not an accident
 * colliding with this one, the two tables answer different questions (kind
 * of window vs. environment), and a window can legitimately be in both. */
const DOCKED_ONLY: ReadonlySet<string> = new Set([
  "lobbyWnd",
  "cloneBay",
  "CloneStationWindow",
  "CloneUpgradeWindow",
  "InventoryStation",
  "InventoryStructure",
  "StructureItemHangar",
  "StructureShipHangar",
  "StructureCorpHangar",
  "DeliverToStructure",
]);

/** Windows that only exist in space. `ShipCargo` and `ShipDroneBay` were
 * considered and deliberately left out: a docked player can open the active
 * ship's cargo hold and drone bay from the station hangar, so they are not
 * space-exclusive at all — including them would have hidden a window the
 * player can genuinely have open while docked, the one direction this table
 * must never fail in. */
const SPACE_ONLY: ReadonlySet<string> = new Set([
  "InventorySpace",
  "droneview",
  "selecteditemview",
  "directionalScannerWindow",
  "overview",
]);

/** Whether a window is shown in `env`. An id is a member of a set if EITHER
 * its exact id or its family is listed, so one entry can cover a family's bare
 * parent and all its spawned instances (`overview` and `overview_1`) — but
 * only for entries whose family is also a PARAM prefix, the same condition
 * `CLUTTER_FAMILIES` documents above (describe() never groups a suffixed id
 * into a family that isn't one). Of the two sets, only `StructureShipHangar`
 * and `overview` qualify; every other entry here is a singleton with no
 * spawned form, so for those the family and exact-id checks below coincide
 * and this is really just an exact-id lookup.
 *
 * No `detail !== ""` check, unlike isClutter: that check tells a spawned
 * instance from its bare parent, and for environment purposes they are in the
 * same place. */
export function inEnv(id: string, env: Env): boolean {
  if (env === "all") return true;
  const family = describe(id).family;
  const has = (s: ReadonlySet<string>) => s.has(id) || s.has(family);
  return env === "docked" ? !has(SPACE_ONLY) : !has(DOCKED_ONLY);
}

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
 * The name to show for a window: EVE's own, when the file carries one, else the
 * one derived from the id. Detail and family always come from the id — they
 * describe the id's shape, which a display name says nothing about.
 */
export function nameOf(w: { id: string; name?: string | null }): WindowName {
  const derived = describe(w.id);
  return w.name ? { ...derived, label: w.name } : derived;
}

/**
 * A stack's own display label — EVE's own string from the account file's
 * `tabgroups`, or `null` when there wasn't one. The backend (`windows.rs`)
 * leaves `container_label` equal to `container_id` in that case rather than
 * signalling absence directly, so equality with the id is exactly "we have
 * nothing" — the caller supplies its own fallback wording (a type marker like
 * "frame", or today's derived id name), rather than this showing a bare id.
 */
export function stackLabel(s: { container_id: string; container_label: string }): string | null {
  return s.container_label !== s.container_id ? s.container_label : null;
}

/** The friendly name as a single string, for places that cannot render the
 * detail as its own element — canvas rectangles, stack tabs, <option> text.
 * The list renders the same two parts as separate spans; both go through
 * `describe`, so they cannot drift. */
export function displayName(id: string): string {
  const n = describe(id);
  return n.detail ? `${n.label} · ${n.detail}` : n.label;
}

/** The `nameOf` equivalent of `displayName`: EVE's own name when the file has
 * one, else the derived one, with `· detail` appended when there is a detail.
 * Same places as `displayName` — canvas rectangles, stack tabs, <option>
 * text — wherever the window might carry a real name and the detail can't
 * render as its own element. Dropping the detail here reintroduces the
 * ambiguity `854b0d7` fixed: two unnamed chat tabs in one stack would both
 * read "Chat". */
export function displayNameOf(w: { id: string; name?: string | null }): string {
  const n = nameOf(w);
  return n.detail ? `${n.label} · ${n.detail}` : n.label;
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
