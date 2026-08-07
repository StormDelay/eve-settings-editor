// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { labelFor } from "./autofill.ts";

import { check } from "./test/check.ts";

// Curated hit: a known People & Places search widget.
check(
  "curated widget gets its friendly name",
  labelFor("/addressbook/content/main/SearchPanel/Container/SingleLineEditText") ===
    "People & Places search",
);

// Curated hit via substring match (the needle appears mid-path).
check(
  "curated needle matches as a substring",
  labelFor("/inventory/content/main/quickFilter/SingleLineEditText") === "Quick Filter",
);

// Derived fallback: an UNCURATED widget must exercise derive() itself —
// strip boilerplate segments, split camelCase, title-case. (Must not match any
// curated needle, or it would never reach derive.)
check(
  "uncurated widget derives a readable label from camelCase",
  labelFor("/someWindow/content/main/mediumTimer/SingleLineEditText") === "Medium Timer",
);

// Real paths, copied from the corpus dump of 290 distinct widgets (2026-07-30).
// These are the ones that recur on nearly every account, and what they used to
// derive is in the comment — the reason the curated list grew.
const REAL: [path: string, label: string][] = [
  // was "Search Edit"
  ["/l_viewstate/l_view_overlays/l_sidePanels/sidePanel/InfoPanelContainer/mainCont/InfoPanelSearch/topCont/headerCont/searchEdit",
   "Info panel search"],
  // was "Header Cont" — the informative segment was two levels up
  ["/l_main/charactersheet/__maincontainer/main/mainCont/panelCont/SkillsPanel/headerCont/SingleLineEditText",
   "Skill search"],
  // was "Input"
  ["/l_main/XmppChatChannels/__maincontainer/topParent/input", "Chat channel to join"],
  // was "Edit Name" / "Edit Description" — which contract field is which
  ["/l_main/createcontract/__maincontainer/main/form/editField/edit_name", "Contract name"],
  ["/l_main/createcontract/__maincontainer/main/form/editField/edit_description", "Contract description"],
  // was "Search Field"
  ["/l_main/market/__maincontainer/main/MarketBase/leftSide/searchAndSettingParent/searchparent/searchField",
   "Market search"],
  // was "Label Edit"
  ["/l_main/bookmarkLocationWindow/__maincontainer/main/mainCont/labelContainer/labelEdit", "Bookmark label"],
  // was "Reason"
  ["/l_main/TransferMoney/__maincontainer/main/centerCont/reason", "Money transfer reason"],
  // was "Subjec Field" (EVE's own typo)
  ["/l_main/newmessage/__maincontainer/topParent/editCont/subjecField", "Mail subject"],
  // was "Edit0"
  ["/l_main/overviewsettings/__maincontainer/main/overviewtabtop/LayoutGrid/Row2_Col1/Container/edit0",
   "Overview tab name"],
  // was "Filter Name Edit"
  ["/l_main/probeScannerFilterEditor/__maincontainer/main/topParent/nameContainer/filterNameEdit",
   "Scan filter name"],
  // already good, and must stay that way
  ["/l_main/RegisterFleetWindow/__maincontainer/main/fleetName", "Fleet name"],
  ["/l_main/('InventoryStation', None)/__maincontainer/main/rightCont/topRightCont2/InvContQuickFilter/quickFilterInputBox",
   "Quick Filter"],
];
for (const [path, label] of REAL) {
  check(`real widget: ${label}`, labelFor(path) === label);
}

// Never empty, even for a degenerate path.
check("empty-ish path never yields an empty label", labelFor("/") !== "");
check("raw-ish path with no useful segment falls back to the raw string",
  labelFor("///") === "///");

