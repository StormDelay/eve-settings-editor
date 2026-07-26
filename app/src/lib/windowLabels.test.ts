// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { describe, groupByFamily, isClutter, displayName } from "./windowLabels.ts";

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

// --- isClutter ---------------------------------------------------------
{
  // a. whole families that only ever exist as spawned instances.
  check("ChatInvitation is clutter", isClutter("ChatInvitation_1111922349"));
  check("ChannelSettingsDlg is clutter", isClutter("ChannelSettingsDlg_fleet_1038711647935"));
  check("mail_readingWnd is clutter", isClutter("mail_readingWnd_380729425"));
  check("groupInfoWnd is clutter", isClutter("groupInfoWnd_494332"));
  check("contactmanagement is clutter", isClutter("contactmanagement_98477766"));
  check("a spawned ShipCargo instance is clutter", isClutter("ShipCargo_1033391582929"));
  check("a spawned ShipDroneBay instance is clutter", isClutter("ShipDroneBay_1033391582929"));
  check("a spawned StructureShipHangar instance is clutter", isClutter("StructureShipHangar_1033391582929"));
  check("a containerWnd instance is clutter", isClutter("containerWnd_1033391582929"));

  // The exact case the developer asked about: a BARE parent window shares
  // its family string with the suffixed one (`describe("ShipCargo").family
  // === "ShipCargo"` too) but must stay visible — it's the player's window,
  // not a spawned instance. Distinguishing on `detail` (empty for a bare id)
  // is the whole point of isClutter, not just family membership.
  check("a bare ShipCargo stays visible", !isClutter("ShipCargo"));
  check("a bare InventoryStation stays visible", !isClutter("InventoryStation"));
  check("a bare InventorySpace stays visible", !isClutter("InventorySpace"));
  check("a bare InventoryStructure stays visible", !isClutter("InventoryStructure"));
  check("a bare containerContentWindow stays visible", !isClutter("containerContentWindow"));

  // b. chat: only private/direct conversations are clutter; standing
  // channels are defined by what they are NOT, so an unrecognised future
  // channel is kept, not hidden (the safe failure direction).
  check("a private chat is clutter", isClutter("chatchannel_private_0ee11e4f970011ea8e789abe94f5b483"));
  check("a player (direct) chat is clutter", isClutter("chatchannel_player_-78564080"));
  check("Local chat is not clutter", !isClutter("chatchannel_local"));
  check("Corp chat is not clutter", !isClutter("chatchannel_corp"));
  check("Alliance chat is not clutter", !isClutter("chatchannel_alliance"));
  check("Fleet chat is not clutter", !isClutter("chatchannel_fleet"));
  check("Incursion chat is not clutter", !isClutter("chatchannel_incursion"));
  check("Invasion chat is not clutter", !isClutter("chatchannel_invasion"));
  check("an unrecognised future channel is kept, not hidden", !isClutter("chatchannel_newthing"));

  // c. one-off transient dialogs, exact id.
  check("setQuantityPopup is clutter", isClutter("setQuantityPopup"));
  check("BugReportingWindow is clutter", isClutter("BugReportingWindow"));
  check("contractEndpointSearch is clutter", isClutter("contractEndpointSearch"));

  // Ordinary windows are never clutter.
  check("market is not clutter", !isClutter("market"));
  check("overview is not clutter", !isClutter("overview"));

  // d. the 2026-07-26 expansion: a representative new exact-id dialog.
  check("enterShipPassword is clutter", isClutter("enterShipPassword"));

  // e. assembleWindow: parent-vs-spawned, same rule as ShipCargo etc.
  check("a spawned assembleWindow instance is clutter", isClutter("assembleWindow_1039455460976"));
  check("a bare assembleWindow stays visible", !isClutter("assembleWindow"));

  // f. bookmarkLocationWindow deliberately lives in BOTH CLUTTER_IDS (bare
  // id, exact-id branch) and CLUTTER_FAMILIES (suffixed id, family branch).
  check("a bare bookmarkLocationWindow is clutter", isClutter("bookmarkLocationWindow"));
  check(
    "a spawned bookmarkLocationWindow instance is clutter",
    isClutter("bookmarkLocationWindow_1026274319209"),
  );

  // g. regression guard: real placeable windows must never be reclassified
  // as clutter by a future edit to the blocklists.
  check("directionalScannerWindow is not clutter", !isClutter("directionalScannerWindow"));
  check("overview_1 is not clutter", !isClutter("overview_1"));
}

// --- displayName -------------------------------------------------------------
// The single-string form used where the label/detail can't render as their
// own elements (canvas rects, stack tabs, <option> text). Must agree with
// what the list renders as separate spans, since both go through describe().
{
  check("displayName joins label and detail with a middot", displayName("chatchannel_local") === "Chat · local");
  check("displayName is just the label when there is no detail", displayName("market") === "Market");
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
