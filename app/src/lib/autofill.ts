// Friendly labels for editHistory widget paths. The keys here are matched as
// substrings of the widget path (paths are long and version-specific, so an
// exact match would be brittle). Anything unmatched derives a label from the
// path; the raw path is always shown by the view, so a miss is never confusing.
const CURATED: [needle: string, label: string][] = [
  ["/addressbook/", "People & Places search"],
  ["quickFilter", "Quick Filter"],
  ["/wallet/", "Wallet transfer reason"],
  ["overviewExport", "Overview export filename"],
  ["/fitting", "Fitting name"],
  ["/fleet", "Fleet name"],
  ["structureBrowser", "Structure browser search"],
  ["skillCatalog", "Skill catalogue search"],
  ["channelName", "Chat channel name"],
  ["bugReport", "Bug report title"],
];

// Path segments that carry no meaning for a human — dropped before deriving.
const BOILERPLATE = new Set(["content", "main", "container", "singlelineedittext", "edittext"]);

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
