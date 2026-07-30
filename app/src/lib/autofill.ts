// Friendly labels for editHistory widget paths. The keys here are matched as
// substrings of the widget path (paths are long and version-specific, so an
// exact match would be brittle). Anything unmatched derives a label from the
// path; the raw path is always shown by the view, so a miss is never confusing.
//
// The list below was expanded on 2026-07-30 against the real corpus: 290
// distinct widget paths across the account files, of which these cover 206 (it
// was 62). The rest derive something readable — "Skins Panel", "Sell Filter",
// "Edit Division3" — which is the bar for leaving one alone. FIRST MATCH WINS,
// so field-level needles come before the window-level ones that would also
// match them.
const CURATED: [needle: string, label: string][] = [
  // --- specific fields, before the windows that contain them ---
  ["edit_endstationname", "Contract destination"],
  ["edit_description", "Contract description"],
  ["reqitemsparent/itemtype", "Contract item type"],
  ["edit_name", "Contract name"],
  ["contractsearch", "Contract search"],
  ["mycontractsparent", "Contract filter"],
  ["row2_col1/container/edit0", "Overview tab name"],
  ["overviewsettings", "Overview settings search"],
  ["filternameedit", "Scan filter name"],
  ["probescannerfiltereditor", "Scan filter search"],
  ["subjecttextedit", "Gift message subject"],
  ["searchbarcontainer/searchinput", "Gift recipient search"],
  ["subjecfield", "Mail subject"], // EVE's own spelling
  // --- windows ---
  ["/addressbook/", "People & Places search"],
  ["xmppchatchannels", "Chat channel to join"],
  ["marketbase", "Market search"],
  ["skillspanel", "Skill search"],
  ["skillcatalog", "Skill catalogue search"],
  ["infopanelsearch", "Info panel search"],
  ["bookmarklocationwindow", "Bookmark label"],
  ["multifitwnd", "Multi-fit name"],
  ["setnewname", "Rename dialog"],
  ["shortcuts_container", "Shortcut search"],
  ["expandedutilmenu", "Utility menu search"],
  ["editmemberdialog", "Corporation member title"],
  ["accountspar", "Corporation member search"],
  ["/assets/", "Assets search"],
  ["mapviewsearch", "Map search"],
  ["standingspanel", "Standings search"],
  ["transfermoney", "Money transfer reason"],
  ["storefleetsetupwnd", "Saved fleet setup name"],
  ["groupswnd", "Groups search"],
  ["structurebrowser", "Structure browser search"],
  ["accessgroupsaddmember", "Access list member search"],
  ["delivertostructure", "Delivery structure search"],
  ["filtercreationwindow", "Filter name"],
  ["pcownerpickerdialog", "Owner search"],
  ["structuredeploymentwndid", "Structure schedule name"],
  ["bookmarkfolderwindow", "Bookmark folder name"],
  ["createcorp", "Corporation name"],
  ["managelabels", "Contact label name"],
  ["bugreport", "Bug report title"],
  ["overviewexport", "Overview export filename"],
  ["quickfilter", "Quick Filter"],
  ["/wallet/", "Wallet transfer reason"],
  ["/fitting", "Fitting name"],
  ["/fleet", "Fleet name"],
  ["channelname", "Chat channel name"],
];

// Path segments that carry no meaning for a human — dropped before deriving.
// The second group is EVE's layout scaffolding, which is what a real path is
// mostly made of: dropping it lets the fallback land on the window or panel
// name instead of on "Header Cont".
const BOILERPLATE = new Set([
  "content",
  "main",
  "container",
  "singlelineedittext",
  "darkstylesinglelineedittext",
  "edittext",
  "__maincontainer",
  "__clipper",
  "__content",
  "maincont",
  "topparent",
  "topcont",
  "headercont",
  "panelcont",
  "centercont",
  "containerautosize",
  "dragresizecont",
  "editfield",
  "form",
  "leftside",
  "rightside",
  "scroll",
]);

export function labelFor(widget: string): string {
  const lower = widget.toLowerCase();
  for (const [needle, label] of CURATED) {
    if (lower.includes(needle.toLowerCase())) return label;
  }
  return derive(widget);
}

function derive(widget: string): string {
  const segments = widget
    .split("/")
    .filter((s) => s.length > 0 && !BOILERPLATE.has(s.toLowerCase()));
  const pick = segments[segments.length - 1];
  if (!pick) return widget; // nothing useful — show the raw path rather than "".
  // Split camelCase / snake into words and title-case them.
  const words = pick
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/[_-]+/g, " ")
    .trim()
    .split(/\s+/);
  return words.map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
}
