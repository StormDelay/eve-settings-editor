//! The client's per-window size limits: each window class's `default_minSize`
//! and `default_fixedWidth`/`default_fixedHeight`, in UI px. Generated from the
//! client's code by `tools/gen-window-sizes.py`; see docs/format-notes.md,
//! "How the client applies a saved rect at a different resolution".

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SizeLimits {
    pub min_w: i64,
    pub min_h: i64,
    #[serde(default)]
    pub fixed_w: Option<i64>,
    #[serde(default)]
    pub fixed_h: Option<i64>,
}

#[derive(Deserialize)]
struct Table {
    ids: HashMap<String, SizeLimits>,
    prefixes: HashMap<String, SizeLimits>,
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("../data/window-sizes.json")).expect("window-sizes.json"))
}

/// A window's limits by the id its rect is saved under. A parameterized window
/// is saved as a stringified tuple, `('name', instance)`, and the client reads
/// its class's settings by the first element — so that is looked up next. Then
/// the longest matching prefix (`chatchannel_<key>`). None when the client's
/// code gave no constant answer: the client still clamps on load.
pub fn window_limits(id: &str) -> Option<SizeLimits> {
    let t = table();
    // `window_layout` keeps ids unique by suffixing a duplicate `#<n>`.
    let id = match id.rsplit_once('#') {
        Some((base, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => id,
    };
    if let Some(l) = t.ids.get(id) {
        return Some(*l);
    }
    if let Some(first) = id.strip_prefix("('").and_then(|r| r.split('\'').next()) {
        return t.ids.get(first).copied();
    }
    t.prefixes.iter().filter(|(p, _)| id.starts_with(p.as_str())).max_by_key(|(p, _)| p.len()).map(|(_, l)| *l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_carries_the_client_constants() {
        assert_eq!(window_limits("overview").map(|l| (l.min_w, l.min_h)), Some((128, 128)));
        assert_eq!(window_limits("market").map(|l| (l.min_w, l.min_h)), Some((650, 435)));
        assert_eq!(window_limits("corporation").map(|l| (l.min_w, l.min_h)), Some((700, 832)));
        assert!(window_limits("fittingWnd").and_then(|l| l.fixed_h).is_some());
        assert_eq!(window_limits("no_such_window"), None);
    }

    #[test]
    fn lookup_handles_tuple_and_prefix_ids() {
        assert_eq!(window_limits("chatchannel_player_90000001").map(|l| (l.min_w, l.min_h)), Some((120, 120)));
        assert_eq!(window_limits("('Inventory', 1000000000001L)").map(|l| (l.min_w, l.min_h)), Some((100, 140)));
        assert_eq!(window_limits("('unknownthing', 1)"), None);
    }

    /// `window_layout` suffixes a duplicate id `#<n>`; it is still that window.
    #[test]
    fn a_deduplicated_id_finds_its_window() {
        assert_eq!(window_limits("market#3").map(|l| (l.min_w, l.min_h)), Some((650, 435)));
        assert_eq!(window_limits("chatchannel_local#7").map(|l| (l.min_w, l.min_h)), Some((120, 120)));
        assert_eq!(window_limits("market#x"), None, "only a numeric suffix is the dedup one");
    }
}
