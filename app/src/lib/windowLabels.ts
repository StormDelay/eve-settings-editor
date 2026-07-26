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
]);

/** True for a window EVE spawns per conversation/item/dialog rather than one
 * the player placed. Hidden in both the list and the canvas, whether open or
 * closed — open/closed is not the axis; kind of window is. */
export function isClutter(id: string): boolean {
  if (CLUTTER_IDS.has(id)) return true;
  const n = describe(id);
  if (n.family === "chatchannel") return CLUTTER_CHAT_DETAILS.has(n.detail);
  // detail === "" means a bare parent window (e.g. plain "ShipCargo") — keep it.
  return CLUTTER_FAMILIES.has(n.family) && n.detail !== "";
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

/** The friendly name as a single string, for places that cannot render the
 * detail as its own element — canvas rectangles, stack tabs, <option> text.
 * The list renders the same two parts as separate spans; both go through
 * `describe`, so they cannot drift. */
export function displayName(id: string): string {
  const n = describe(id);
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
