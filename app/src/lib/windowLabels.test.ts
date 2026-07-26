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
