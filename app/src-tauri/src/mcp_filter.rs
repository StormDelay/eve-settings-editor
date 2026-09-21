//! The canvas's view filters, for `layout_get`/`layout_render`: clutter (windows
//! EVE spawns per conversation, item or dialog), environment (docked vs in
//! space), open-only, and text — over the same tables `windowLabels.ts` reads
//! from `data/window-filters.json`. View filters only: `layout_edit` reaches
//! any window. Mirrors `isClutter`, `inEnv`, `isOrphanFrame`, `windowMatches`.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use settings_model::WindowRect;

use crate::prefs::Preferences;

/// The tables `windowLabels.ts` also reads. One file, two readers.
const FILTERS_JSON: &str = include_str!("../../src/lib/data/window-filters.json");

#[derive(Deserialize)]
struct Tables {
    param: HashMap<String, String>,
    clutter_families: Vec<String>,
    clutter_chat_details: Vec<String>,
    clutter_ids: Vec<String>,
    docked_only: Vec<String>,
    space_only: Vec<String>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(FILTERS_JSON).expect("window-filters.json"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Env {
    #[default]
    All,
    Docked,
    Space,
}

/// The user's own clutter decisions, from preferences.json.
#[derive(Debug, Default)]
pub(crate) struct Overrides {
    pub clutter: HashSet<String>,
    pub visible: HashSet<String>,
    /// How many locked targets `layout_render` draws the target list at.
    pub targets: u8,
}

impl Overrides {
    pub(crate) fn from_prefs(p: &Preferences) -> Self {
        Overrides {
            clutter: p.layout.clutter.iter().cloned().collect(),
            visible: p.layout.visible.iter().cloned().collect(),
            targets: p.layout.targets,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Family {
    pub family: String,
    pub detail: String,
}

/// `windowLabels.ts`'s OPAQUE: an id, a hash, a GUID — a suffix segment that
/// carries no meaning for a reader.
fn opaque(seg: &str) -> bool {
    let s = seg.strip_prefix('-').unwrap_or(seg);
    let s = s.strip_suffix('L').unwrap_or(s);
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        || (seg.len() >= 16 && seg.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// `instanceDetail`: the leading non-opaque segments of a suffix, or the
/// suffix itself when every segment is opaque.
fn instance_detail(rest: &str) -> String {
    let kept: Vec<&str> = rest.split('_').take_while(|seg| !opaque(seg)).collect();
    if kept.is_empty() { rest.to_string() } else { kept.join(" ") }
}

/// `describe()`'s family and detail — the label is the projection's job.
pub(crate) fn describe(id: &str) -> Family {
    // 1. A stringified Python tuple: ('corpassets', 1037014783783L).
    if let Some(rest) = id.strip_prefix("('") {
        if let Some(end) = rest.find('\'') {
            let family = &rest[..end];
            let detail = rest[end + 1..].trim_start_matches([',', ' ']).trim_end_matches(')').trim();
            return Family { family: family.into(), detail: detail.into() };
        }
    }
    // 2. All digits: a stack container EVE minted.
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        return Family { family: "stack".into(), detail: id.into() };
    }
    // 3. A parameterised family, longest prefix first. `strip_prefix` avoids
    // allocating a `format!("{p}_")` per candidate per call.
    let best = tables().param.keys().filter(|p| id.strip_prefix(p.as_str()).is_some_and(|r| r.starts_with('_'))).max_by_key(|p| p.len());
    if let Some(prefix) = best {
        return Family { family: prefix.clone(), detail: instance_detail(&id[prefix.len() + 1..]) };
    }
    // 4/5. A singleton.
    Family { family: id.into(), detail: String::new() }
}

/// `isClutter`: overrides first, then exact ids, then chat by detail, then a
/// spawned instance of a clutter family (a bare parent stays visible).
pub(crate) fn is_clutter(id: &str, o: &Overrides) -> bool {
    if o.visible.contains(id) { return false; }
    if o.clutter.contains(id) { return true; }
    let t = tables();
    if t.clutter_ids.iter().any(|c| c == id) { return true; }
    let f = describe(id);
    if f.family == "chatchannel" {
        return t.clutter_chat_details.contains(&f.detail);
    }
    t.clutter_families.contains(&f.family) && !f.detail.is_empty()
}

/// A minted numeric container that belongs to no stack — a dead frame.
pub(crate) fn is_orphan_frame(w: &WindowRect) -> bool {
    w.stack.is_none() && !w.id.is_empty() && w.id.bytes().all(|b| b.is_ascii_digit())
}

/// `inEnv`: only the exclusives hide; an id is in a set by exact id or family.
pub(crate) fn in_env(id: &str, env: Env) -> bool {
    if env == Env::All { return true; }
    let family = describe(id).family;
    let t = tables();
    let has = |set: &[String]| set.iter().any(|s| *s == id || *s == family);
    match env {
        Env::Docked => !has(&t.space_only),
        Env::Space => !has(&t.docked_only),
        Env::All => true,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WindowFilter {
    pub include_closed: bool,
    pub hide_clutter: bool,
    pub env: Env,
    pub matches: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hidden { Closed, Clutter, Environment, Match }

/// Why a window is left out of the view, or `None` if it is shown. The order
/// is the canvas's `windowMatches`: open, clutter, environment, text.
pub(crate) fn hidden_by(w: &WindowRect, f: &WindowFilter, o: &Overrides) -> Option<Hidden> {
    if !f.include_closed && !w.open { return Some(Hidden::Closed); }
    if f.hide_clutter && (is_clutter(&w.id, o) || is_orphan_frame(w)) { return Some(Hidden::Clutter); }
    if !in_env(&w.id, f.env) { return Some(Hidden::Environment); }
    if let Some(q) = f.matches.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let q = q.to_lowercase();
        // `label` is the backend's raw id (`windows.rs` only fills in `name`
        // for a resolved chat channel) — search the resolved name when there
        // is one, same as `nameOf`/`layout_get` shows it, so a chat the user
        // asks about by its real name is found.
        let shown_label = w.name.as_deref().unwrap_or(&w.label);
        let hay = format!("{} {} {}", shown_label, describe(&w.id).detail, w.id).to_lowercase();
        if !hay.contains(&q) { return Some(Hidden::Match); }
    }
    None
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub(crate) struct HiddenCounts { pub closed: usize, pub clutter: usize, pub environment: usize, pub matched: usize }

impl HiddenCounts {
    pub(crate) fn add(&mut self, h: Hidden) {
        match h {
            Hidden::Closed => self.closed += 1,
            Hidden::Clutter => self.clutter += 1,
            Hidden::Environment => self.environment += 1,
            Hidden::Match => self.matched += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_splits_families_details_tuples_and_stacks() {
        assert_eq!(describe("ShipCargo_1033391582929"), Family { family: "ShipCargo".into(), detail: "1033391582929".into() });
        assert_eq!(describe("chatchannel_player_-78564080").family, "chatchannel");
        assert_eq!(describe("chatchannel_player_-78564080").detail, "player");
        assert_eq!(describe("chatchannel_private_0ee11e4f970011ea8e789abe94f5b483").detail, "private");
        assert_eq!(describe("ShipCargo").detail, "");
        assert_eq!(describe("('corpassets', 1037014783783L)").family, "corpassets");
        assert_eq!(describe("7001"), Family { family: "stack".into(), detail: "7001".into() });
        assert_eq!(describe("market").family, "market");
    }

    #[test]
    fn clutter_matches_the_frontend_rules() {
        let o = Overrides::default();
        for id in ["ChatInvitation_1111922349", "ChannelSettingsDlg_fleet_1038711647935", "mail_readingWnd_380729425", "groupInfoWnd_494332", "contactmanagement_98477766", "ShipCargo_1033391582929", "ShipDroneBay_1033391582929", "StructureShipHangar_1033391582929", "containerWnd_1033391582929", "chatchannel_private_0ee11e4f970011ea8e789abe94f5b483", "chatchannel_player_-78564080", "setQuantityPopup", "BugReportingWindow", "contractEndpointSearch", "enterShipPassword", "assembleWindow_1039455460976"] {
            assert!(is_clutter(id, &o), "{id} should be clutter");
        }
        for id in ["ShipCargo", "InventoryStation", "InventorySpace", "InventoryStructure", "containerContentWindow", "chatchannel_local", "chatchannel_corp", "chatchannel_alliance", "chatchannel_fleet", "chatchannel_incursion", "chatchannel_invasion", "chatchannel_newthing", "market", "overview", "probeScannerWindow", "assembleWindow"] {
            assert!(!is_clutter(id, &o), "{id} should not be clutter");
        }
    }

    #[test]
    fn overrides_win_in_both_directions() {
        let mut o = Overrides::default();
        o.visible.insert("ShipCargo_1033391582929".into());
        o.clutter.insert("market".into());
        assert!(!is_clutter("ShipCargo_1033391582929", &o), "forced visible");
        assert!(is_clutter("market", &o), "forced into the clutter set");
    }

    #[test]
    fn environments_hide_only_the_exclusives() {
        assert!(in_env("lobbyWnd", Env::All) && in_env("overview", Env::All));
        assert!(in_env("lobbyWnd", Env::Docked) && !in_env("lobbyWnd", Env::Space));
        assert!(in_env("StructureItemHangar", Env::Docked) && !in_env("StructureItemHangar", Env::Space));
        assert!(in_env("overview", Env::Space) && !in_env("overview", Env::Docked));
        assert!(in_env("overview_1", Env::Space) && !in_env("overview_1", Env::Docked), "a spawned instance follows its family");
        assert!(in_env("directionalScannerWindow", Env::Space));
        assert!(in_env("market", Env::Docked) && in_env("market", Env::Space), "unlisted shows in both");
    }

    /// `windows.rs` sets `label` to the raw id and only fills `name` for a
    /// resolved chat channel — `layout_get` shows `name` when there is one
    /// (`nameOf`'s rule), so `match` must search it too, not just the raw id.
    #[test]
    fn a_match_searches_the_resolved_name_not_just_the_raw_id() {
        let w = WindowRect {
            id: "chatchannel_player_123".into(),
            label: "chatchannel_player_123".into(),
            name: Some("Fleet Ops".into()),
            open: true,
            renderable: true,
            resolution_matches: true,
            geom: None,
            flags: Vec::new(),
            stack: None,
        };
        let f = WindowFilter { include_closed: true, hide_clutter: false, env: Env::All, matches: Some("fleet".into()) };
        assert_eq!(hidden_by(&w, &f, &Overrides::default()), None, "the resolved name matches, even though the raw id does not contain it");
        let f = WindowFilter { include_closed: true, hide_clutter: false, env: Env::All, matches: Some("nomatch".into()) };
        assert_eq!(hidden_by(&w, &f, &Overrides::default()), Some(Hidden::Match));
    }
}
