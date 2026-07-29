//! Structural authoring for overview tabs: edit the user file's `overview`
//! container — `tabsettings_new` (index-keyed tab dict) and
//! `tabsByWindowInstanceID` (window -> tab indices). Window-id/name keys and
//! tab tokens are `Shared`/`Ref` on real files, so every entry point inlines
//! the whole tree first (drops all sharing) and edits plain values; the app
//! layer reshares before saving. Mirrors stacks.rs / overview.rs.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{inline_all, key_is, Entries};

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum OverviewTabError {
    /// No `overview` container in the file.
    NoOverview,
    /// No tab with this index in `tabsettings_new`.
    UnknownTab { index: i64 },
    /// No overview window at this position in `tabsByWindowInstanceID`.
    UnknownWindow { index: usize },
    /// Refused: would delete the last remaining tab.
    LastTab,
    /// Refused: would remove the last overview window.
    LastWindow,
    /// Refused: this account has no tab-to-window mapping. NOT damage — EVE's
    /// own overview importer deletes `tabsByWindowInstanceID` (confirmed
    /// 2026-07-28) and the client distributes tabs across its char-side windows
    /// by default. `create_window_mapping` is the deliberate way out.
    NoWindowMapping,
    /// Refused: this account already maps tabs to windows.
    WindowMappingExists,
    /// Refused: there are no tabs to map, and a mapping whose window lists no
    /// tabs hides the entire overview.
    NoTabsToMap,
    /// Refused: only the last overview window can be removed for now.
    NotLastWindow { index: usize },
    /// No preset with this name in `overviewProfilePresets`.
    UnknownPreset { name: String },
    /// A preset with the target name already exists.
    PresetExists { name: String },
    /// Refused: would delete the last remaining preset.
    LastPreset,
    /// Key is not in the boolean-settings allow-list (`OVERVIEW_BOOLS`).
    UnknownSetting { key: String },
}

impl std::fmt::Display for OverviewTabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverviewTabError::NoOverview => write!(f, "This file has no overview settings."),
            OverviewTabError::UnknownTab { index } => write!(f, "Tab {index} does not exist."),
            OverviewTabError::UnknownWindow { index } => write!(f, "Overview window {index} does not exist."),
            OverviewTabError::LastTab => write!(f, "An overview must keep at least one tab."),
            OverviewTabError::LastWindow => write!(f, "There must be at least one overview window."),
            OverviewTabError::NoWindowMapping => write!(f, "This account does not use per-window tabs, so there are no windows to change. EVE removes the tab-to-window mapping whenever an overview pack is imported through the client, and the overview works normally without it."),
            OverviewTabError::WindowMappingExists => write!(f, "This account already uses per-window tabs."),
            OverviewTabError::NoTabsToMap => write!(f, "There are no overview tabs to map to a window."),
            OverviewTabError::NotLastWindow { index } => write!(f, "Only the last overview window can be removed (tried {index})."),
            OverviewTabError::UnknownPreset { name } => write!(f, "Preset \"{name}\" does not exist."),
            OverviewTabError::PresetExists { name } => write!(f, "A preset named \"{name}\" already exists."),
            OverviewTabError::LastPreset => write!(f, "An overview must keep at least one preset."),
            OverviewTabError::UnknownSetting { key } => write!(f, "Setting \"{key}\" is not recognised."),
        }
    }
}

pub(crate) fn is_b(k: &Value, name: &[u8]) -> bool {
    matches!(k, Value::Bytes(b) if b.as_slice() == name)
}

pub(crate) fn as_int(v: &Value) -> Option<i64> {
    match v { Value::Int(i) => Some(*i), _ => None }
}

/// The tab EVE gets when there is no sibling to clone from (an empty overview
/// — reachable when the account has zero tabs, whether via `create_tab`'s
/// no-sibling path or `overview_pack.rs`'s zero-tab pack-apply path). Every
/// real EVE tab carries `bracket` and `color` — EVE's "reset all overview
/// settings" iterates tabs reading both — so this fallback must too; a single
/// shared definition means both call sites can't drift apart on the default.
pub(crate) fn fallback_tab() -> Value {
    Value::Dict(vec![
        (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
        (Value::Bytes(b"color".to_vec()), Value::None),
        (Value::Bytes(b"overview".to_vec()), Value::Bytes(Vec::new())),
    ])
}

/// Inner dict of a plain (post-inline) value, unwrapping a `(ts, dict)` tuple.
/// The read-only counterpart to `dict_inner_mut`, for callers — including this
/// module's own tests — that must not create what they cannot find.
pub(crate) fn dict_inner(v: &Value) -> Option<&Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
        _ => None,
    }
}

pub(crate) fn dict_inner_mut(v: &mut Value) -> Option<&mut Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

/// Restore the `(timestamp, payload)` wrapper on a container key that lost it.
///
/// A bare payload is not a shape the client writes — 0 of 4,187 container keys
/// across five untouched account files. One can only be there because an older
/// build of this editor stripped the wrapper, which `tests/overview_tabs_corpus.rs`
/// now confirms rather than assumes: of 134 real account files it edits, the
/// single one carrying a bare container is an editor-written snapshot, and every
/// untouched baseline of that same account is wrapped. So a write passing through
/// here repairs it rather than perpetuating it.
///
/// Narrower than `overview_pack::put`, deliberately: `put` replaces the payload
/// so it can wrap anything, including `Value::None`; this KEEPS the payload, and
/// wrapping a `None` would only produce a `Tuple(Long, None)` that the caller's
/// unwrap still rejects. Matching `Dict | List` is the whole of what is
/// repairable here.
///
/// An existing wrapper is left alone, timestamp and all: the repair is for a
/// MISSING wrapper, and resetting a real timestamp to zero would be a different
/// kind of damage.
fn rewrap(slot: &mut Value) {
    if matches!(slot, Value::Dict(_) | Value::List(_)) {
        let inner = std::mem::replace(slot, Value::None);
        *slot = Value::Tuple(vec![Value::Long(vec![0u8; 8]), inner]);
    }
}

/// Inner list of a plain (post-inline) value, unwrapping a `(ts, list)` tuple.
/// `pub(crate)` so `overview_pack.rs`'s window re-pointing can reuse the same
/// unwrap rather than writing it out again.
pub(crate) fn list_inner_mut(v: &mut Value) -> Option<&mut Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

/// Read-only counterpart of `list_inner_mut`, unwrapping a `(ts, list)` tuple.
fn list_inner(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

/// Mutable `overview` container dict (tree already inlined).
pub(crate) fn overview_mut(v: &mut Value) -> Result<&mut Entries, OverviewTabError> {
    let Value::Dict(root) = v else { return Err(OverviewTabError::NoOverview) };
    let (_, ov) = root.iter_mut().find(|(k, _)| is_b(k, b"overview")).ok_or(OverviewTabError::NoOverview)?;
    dict_inner_mut(ov).ok_or(OverviewTabError::NoOverview)
}

/// Mutable tab dict under `tabsettings_new`, migrating a legacy `tabsettings`
/// key first (the two are structurally identical; EVE reads `tabsettings_new`).
/// Created empty if neither key exists.
pub(crate) fn tabs_mut(ov: &mut Entries) -> &mut Entries {
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsettings_new")) {
        if let Some((k, _)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsettings")) {
            *k = Value::Bytes(b"tabsettings_new".to_vec());
        }
    }
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsettings_new")) {
        // `(timestamp, payload)` — the file-wide wrapper convention, and a zero
        // Long is what every other create-from-absent path mints (see
        // `hud.rs`). Creating a bare Dict here produced a shape no client has
        // ever written, on exactly the accounts least able to cope with it: a
        // brand-new install, or one whose tab key was deleted.
        ov.push((
            Value::Bytes(b"tabsettings_new".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
    rewrap(v);
    dict_inner_mut(v).expect("tabsettings_new is a dict or (ts,dict)")
}

/// Mutable window-groups list under `tabsByWindowInstanceID`. Created empty if absent.
fn groups_mut(ov: &mut Entries) -> &mut Vec<Value> {
    if !ov.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) {
        ov.push((
            Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::List(Vec::new())]),
        ));
    }
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
    rewrap(v);
    list_inner_mut(v).expect("tabsByWindowInstanceID is a list or (ts,list)")
}

/// Set a tab's name, preserving an existing name entry's value variant (real
/// files store names as Str / StrUcs2 / Bytes), inserting a plain `name` key
/// (unicode-safe `StrUcs2`) if the tab has none. The name KEY may itself be a
/// string-table token (`StrTable(52)`); we match it the same way the reader does.
fn set_name(fields: &mut Entries, name: &str) {
    if let Some((_, val)) = fields.iter_mut().find(|(k, _)| key_is(k, "name")) {
        *val = match val {
            Value::Bytes(_) => Value::Bytes(name.as_bytes().to_vec()),
            Value::Str(_) => Value::Str(name.to_string()),
            _ => Value::StrUcs2(name.to_string()),
        };
        return;
    }
    fields.push((Value::Str("name".into()), Value::StrUcs2(name.to_string())));
}

/// True if a dict key is the tab-name key, whether stored as `Str("name")`,
/// `Bytes("name")`, or the string-table token `StrTable(52)` real files use.
/// How many overview windows the account's `tabsByWindowInstanceID` maps tabs to
/// (0 when the mapping is absent — a windowless account). Reading it never
/// fabricates the mapping.
fn window_count(ov: &Entries) -> usize {
    ov.iter()
        .find(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
        .and_then(|(_, wv)| list_inner(wv))
        .map_or(0, |g| g.len())
}

pub fn rename_tab(v: &mut Value, tab_idx: i64, name: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let tabs = tabs_mut(ov);
    let (_, tab) = tabs.iter_mut().find(|(k, _)| as_int(k) == Some(tab_idx))
        .ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    let fields = dict_inner_mut(tab).ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    set_name(fields, name);
    Ok(())
}

/// Point a tab at a filter preset by name (its `overview` field). Stores the
/// name as `Bytes`, matching real files; inserts the key if the tab lacks it.
pub fn set_tab_preset(v: &mut Value, tab_idx: i64, preset: &str) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    let tabs = tabs_mut(ov);
    let (_, tab) = tabs.iter_mut().find(|(k, _)| as_int(k) == Some(tab_idx))
        .ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    let fields = dict_inner_mut(tab).ok_or(OverviewTabError::UnknownTab { index: tab_idx })?;
    if let Some((_, val)) = fields.iter_mut().find(|(k, _)| is_b(k, b"overview")) {
        *val = Value::Bytes(preset.as_bytes().to_vec());
    } else {
        fields.push((Value::Bytes(b"overview".to_vec()), Value::Bytes(preset.as_bytes().to_vec())));
    }
    Ok(())
}

/// Create a new tab by CLONING a sibling (`from_tab`, else the first tab) and
/// overriding its name. Cloning — rather than building a minimal `{name,
/// overview}` dict — is required: every real EVE tab carries `bracket` and
/// `color` keys, and EVE's "reset all overview settings" iterates tabs reading
/// them, so a tab missing them makes the reset throw. The clone also inherits
/// the sibling's preset (`overview`) and its name-key encoding; its column
/// lists are dropped so the new tab inherits columns.
pub fn create_tab(v: &mut Value, window_idx: usize, name: &str, from_tab: Option<i64>) -> Result<i64, OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // How many overview windows the account maps tabs to. EVE honors an explicit
    // tab->window mapping only when `tabsByWindowInstanceID` exists with ≥1
    // window; absent/empty means EVE distributes tabs across its (char-side)
    // overview windows by default. We must NOT create or touch the mapping in
    // that case — the per-window distribution is char-side state we can't
    // reconstruct here, and a partial/wrong mapping hides the whole overview.
    let window_count = window_count(ov);
    if window_count > 0 && window_idx >= window_count {
        return Err(OverviewTabError::UnknownWindow { index: window_idx });
    }
    let new_idx = {
        let tabs = tabs_mut(ov);
        let new_idx = tabs.iter().filter_map(|(k, _)| as_int(k)).max().map(|m| m + 1).unwrap_or(0);
        let template = from_tab
            .and_then(|t| tabs.iter().position(|(k, _)| as_int(k) == Some(t)))
            .or(if tabs.is_empty() { None } else { Some(0) });
        let mut tab = match template {
            Some(i) => tabs[i].1.clone(),
            // Last-resort tab when there is no sibling to clone (an empty
            // overview — only reachable when the account has no tabs). Still
            // carries bracket/color so the "every created tab is a valid EVE
            // tab" invariant holds on this path too.
            None => fallback_tab(),
        };
        if let Some(fields) = dict_inner_mut(&mut tab) {
            fields.retain(|(k, _)| !is_b(k, b"tabColumnOrder") && !is_b(k, b"tabColumns"));
            set_name(fields, name);
        }
        tabs.push((Value::Int(new_idx), tab));
        new_idx
    };
    // Attach to the named window ONLY when the account has an explicit mapping;
    // otherwise the tab lives in tabsettings_new and EVE shows it by default.
    if window_count > 0 {
        if let Some((_, wv)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) {
            if let Some(inner) = list_inner_mut(wv).and_then(|g| g.get_mut(window_idx)).and_then(list_inner_mut) {
                inner.push(Value::Int(new_idx));
            }
        }
    }
    Ok(new_idx)
}

pub fn delete_tab(v: &mut Value, tab_idx: i64) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    {
        let tabs = tabs_mut(ov);
        if !tabs.iter().any(|(k, _)| as_int(k) == Some(tab_idx)) {
            return Err(OverviewTabError::UnknownTab { index: tab_idx });
        }
        if tabs.len() <= 1 {
            return Err(OverviewTabError::LastTab);
        }
        tabs.retain(|(k, _)| as_int(k) != Some(tab_idx));
    }
    // Purge the index from every window strip — but only if a mapping already
    // exists. Do NOT fabricate an (empty) `tabsByWindowInstanceID` when it's
    // absent: EVE keys the overview off it, and an empty mapping can hide the
    // whole overview (matches create_tab's no-fabricate behavior).
    if let Some((_, wv)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) {
        if let Some(groups) = list_inner_mut(wv) {
            for g in groups.iter_mut() {
                if let Some(inner) = list_inner_mut(g) {
                    inner.retain(|e| as_int(e) != Some(tab_idx));
                }
            }
        }
    }
    Ok(())
}

pub fn reorder_tabs_in_window(v: &mut Value, window_idx: usize, order: &[i64]) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // `groups_mut` CREATES the mapping when it is absent, so this guard has to
    // come first: on a windowless account the call below would refuse the edit
    // and still leave an empty `tabsByWindowInstanceID` behind — which hides the
    // account's whole overview. A refused edit must not FABRICATE a container.
    //
    // Note the precise claim. A refused edit can still normalise the document —
    // every entry point inlines first, and `tabs_mut`/`groups_mut` repair a lost
    // wrapper on the way through. Both are shape-preserving and change nothing
    // the client reads. What must never happen on a refusal is a container
    // coming into existence, because an empty or partial one hides real data.
    if window_count(ov) == 0 {
        return Err(OverviewTabError::NoWindowMapping);
    }
    let inner = groups_mut(ov).get_mut(window_idx).and_then(list_inner_mut)
        .ok_or(OverviewTabError::UnknownWindow { index: window_idx })?;
    *inner = order.iter().map(|&i| Value::Int(i)).collect();
    Ok(())
}

pub fn move_tab(v: &mut Value, tab_idx: i64, from_window: usize, to_window: usize, pos: usize) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // Same as `reorder_tabs_in_window`: `groups_mut` below would fabricate an
    // empty mapping on a windowless account even though the edit is refused.
    if window_count(ov) == 0 {
        return Err(OverviewTabError::NoWindowMapping);
    }
    // Without this, a nonexistent index is simply inserted into the destination
    // strip and the window then points at a tab that does not exist. The UI only
    // ever moves a tab it drew, so this matches `delete_tab`'s guard rather than
    // fixing an observed bug.
    if !tabs_mut(ov).iter().any(|(k, _)| as_int(k) == Some(tab_idx)) {
        return Err(OverviewTabError::UnknownTab { index: tab_idx });
    }
    // Validate the destination window exists BEFORE mutating the source strip,
    // so an invalid to_window can't remove the tab from both windows.
    if groups_mut(ov).get_mut(to_window).and_then(list_inner_mut).is_none() {
        return Err(OverviewTabError::UnknownWindow { index: to_window });
    }
    {
        let src = groups_mut(ov).get_mut(from_window).and_then(list_inner_mut)
            .ok_or(OverviewTabError::UnknownWindow { index: from_window })?;
        src.retain(|e| as_int(e) != Some(tab_idx));
    }
    let dst = groups_mut(ov).get_mut(to_window).and_then(list_inner_mut)
        .ok_or(OverviewTabError::UnknownWindow { index: to_window })?;
    let at = pos.min(dst.len());
    dst.insert(at, Value::Int(tab_idx));
    Ok(())
}

/// Give an account an explicit tab-to-window mapping: one window listing every
/// tab it has, in ascending tab index.
///
/// This is the ONLY path in the codebase allowed to create
/// `tabsByWindowInstanceID`, and it exists because the absent state is normal —
/// EVE's own overview importer deletes the key (confirmed 2026-07-28) and the
/// client then distributes tabs across its char-side windows by default. That
/// default is char-side state this crate cannot read, so writing a mapping
/// REPLACES it: every tab is pinned into one window until the user rearranges
/// them. Destructive enough that it must never be implicit — the UI puts it
/// behind a confirm that says so, and nothing else calls it.
///
/// Completeness is the safety property. A mapping that omits a tab hides that
/// tab, and one that omits all of them hides the whole overview, so this either
/// lists every tab or refuses.
pub fn create_window_mapping(v: &mut Value) -> Result<usize, OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    if window_count(ov) > 0 {
        return Err(OverviewTabError::WindowMappingExists);
    }
    // Read the tabs WITHOUT tabs_mut, which mints an empty `tabsettings_new`
    // when the key is absent: a refused create must leave the file untouched,
    // the same rule the reorder/move guards above exist for.
    let Some(tabs) = ov
        .iter()
        .find(|(k, _)| is_b(k, b"tabsettings_new") || is_b(k, b"tabsettings"))
        .and_then(|(_, v)| dict_inner(v))
    else {
        return Err(OverviewTabError::NoTabsToMap);
    };

    // Ascending index, not dict order. `project_overview` reports tabs in dict
    // order, which is ascending on any file the client wrote (`create_tab`
    // appends `max+1`) but need not be on a hand-edited one — so this is a
    // deliberate normalisation, not a restatement of what the strip shows.
    let mut indices: Vec<i64> = tabs.iter().filter_map(|(k, _)| as_int(k)).collect();
    indices.sort_unstable();
    if indices.is_empty() {
        return Err(OverviewTabError::NoTabsToMap);
    }
    // Completeness is the whole safety property, so a tab this cannot name is a
    // refusal, never a silent omission: `as_int` takes only `Value::Int`, and a
    // dropped key would be a tab hidden in game that the editor still lists.
    if indices.len() != tabs.len() {
        return Err(OverviewTabError::NoTabsToMap);
    }
    // groups_mut creates the key in the wrapped `(timestamp, list)` shape every
    // real file uses. Only reached once the refusals above have passed, so it
    // can never leave a partial mapping behind.
    let groups = groups_mut(ov);
    groups.push(Value::List(indices.iter().map(|&i| Value::Int(i)).collect()));
    Ok(indices.len())
}

/// Add a new overview window (user-file grouping half). Appends an empty inner
/// list to `tabsByWindowInstanceID` and seeds it with one cloned tab (a window
/// must have ≥1 tab).
///
/// Refuses on an account with no mapping at all. That is not a damaged file: EVE
/// deletes the key on every pack import, and distributes tabs across its
/// char-side windows by default. Adding positionally there would fabricate a
/// PARTIAL mapping listing only the new tab, which hides every other tab the
/// account has. `create_window_mapping` writes a complete one instead, and is
/// the only path allowed to create the key.
///
/// Returns the new window's index, always ≥1 here (an account with no mapping is
/// refused with `NoWindowMapping`), so the char key is `overview_{idx}`.
pub fn add_overview_window(v: &mut Value, name: &str, from_tab: Option<i64>) -> Result<usize, OverviewTabError> {
    inline_all(v);
    let new_window_idx = {
        let ov = overview_mut(v)?;
        let window_count = window_count(ov);
        if window_count == 0 {
            return Err(OverviewTabError::NoWindowMapping);
        }
        let groups = groups_mut(ov);
        groups.push(Value::List(Vec::new()));
        groups.len() - 1
    };
    // Seed the new (empty) window with one cloned tab. create_tab re-inlines (a
    // no-op on the already-plain tree) and appends the tab to window `new_window_idx`.
    create_tab(v, new_window_idx, name, from_tab)?;
    Ok(new_window_idx)
}

/// Remove an overview window (user-file grouping half). Reassigns the window's
/// tabs onto window 0 (no tab loss), then drops the inner list. Last-window-only:
/// the positional link to the char-file `overview_N` keys makes middle removal a
/// re-key cascade (deferred).
pub fn remove_overview_window(v: &mut Value, window_idx: usize) -> Result<(), OverviewTabError> {
    inline_all(v);
    let ov = overview_mut(v)?;
    // Read the mapping WITHOUT fabricating it (a windowless account has none).
    let groups = ov.iter_mut()
        .find(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
        .and_then(|(_, wv)| list_inner_mut(wv))
        .ok_or(OverviewTabError::LastWindow)?;
    let count = groups.len();
    if count <= 1 {
        return Err(OverviewTabError::LastWindow);
    }
    if window_idx >= count {
        return Err(OverviewTabError::UnknownWindow { index: window_idx });
    }
    if window_idx != count - 1 {
        return Err(OverviewTabError::NotLastWindow { index: window_idx });
    }
    let removed: Vec<Value> = list_inner(&groups[window_idx]).cloned().unwrap_or_default();
    if let Some(w0) = groups.get_mut(0).and_then(list_inner_mut) {
        w0.extend(removed);
    }
    groups.remove(window_idx);
    Ok(())
}

/// Each new overview window is offset from the primary so it does not land
/// exactly on top of it.
const OVERVIEW_WINDOW_OFFSET: i64 = 40;

/// Char-file: mint the window `overview_{window_idx}` by cloning the primary
/// `overview` window's value in every `windows` subdict (geometry + all flag
/// dicts) and offsetting the new window's on-screen position so it doesn't sit
/// exactly on the primary. Cloning the primary makes the required-flag set
/// correct by construction. `window_idx` must be ≥1 (0 IS the primary key). No-op
/// when there is no `windows` dict, or no primary entry in a given subdict;
/// idempotent (skips a subdict that already has the key).
pub fn add_overview_window_geometry(v: &mut Value, window_idx: usize) {
    if window_idx == 0 {
        return;
    }
    inline_all(v);
    let key = format!("overview_{window_idx}");
    let Value::Dict(root) = v else { return };
    let Some((_, wins)) = root.iter_mut().find(|(k, _)| is_b(k, b"windows")) else { return };
    let Value::Dict(subdicts) = wins else { return };
    for (subkey, subval) in subdicts.iter_mut() {
        let is_geom = is_b(subkey, b"windowSizesAndPositions_1");
        let Some(entries) = dict_inner_mut(subval) else { continue };
        if entries.iter().any(|(k, _)| is_b(k, key.as_bytes())) {
            continue;
        }
        let Some(prim) = entries.iter()
            .find(|(k, _)| is_b(k, b"overview"))
            .map(|(_, val)| val.clone()) else { continue };
        let mut newval = prim;
        if is_geom {
            if let Value::Tuple(items) = &mut newval {
                if let Some(x) = items.get_mut(0) { offset_coord(x, OVERVIEW_WINDOW_OFFSET); }
                if let Some(y) = items.get_mut(1) { offset_coord(y, OVERVIEW_WINDOW_OFFSET); }
            }
        }
        entries.push((Value::Bytes(key.as_bytes().to_vec()), newval));
    }
}

/// Char-file inverse of `add_overview_window_geometry`: drop `overview_{window_idx}`
/// from every `windows` subdict. No-op for `window_idx == 0` or when absent.
pub fn remove_overview_window_geometry(v: &mut Value, window_idx: usize) {
    if window_idx == 0 {
        return;
    }
    inline_all(v);
    let key = format!("overview_{window_idx}");
    let Value::Dict(root) = v else { return };
    let Some((_, wins)) = root.iter_mut().find(|(k, _)| is_b(k, b"windows")) else { return };
    let Value::Dict(subdicts) = wins else { return };
    for (_, subval) in subdicts.iter_mut() {
        if let Some(entries) = dict_inner_mut(subval) {
            entries.retain(|(k, _)| !is_b(k, key.as_bytes()));
        }
    }
}

/// Bump an integer geometry coordinate by `delta`. Coords are `Int` on real
/// files; any other variant is left unchanged (the window overlaps the primary
/// and the user drags it — acceptable, never wrong).
fn offset_coord(v: &mut Value, delta: i64) {
    if let Value::Int(n) = v {
        *n += delta;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    /// A zero timestamp, as every `(timestamp, payload)` container wrapper in
    /// these fixtures carries. Real files hold a real one; nothing here reads it.
    fn ts() -> Value {
        Value::Long(vec![0u8; 8])
    }

    /// user tree: overview -> tabsettings_new `(ts, dict)` -> {0:{bracket,color,name,overview:"P"}}
    /// Both containers are wrapped because that is the only shape EVE writes —
    /// 0 of 4,187 container keys across five untouched account files are bare.
    /// The `bracket`/`color` keys mirror real EVE tabs — every real tab carries
    /// them, and a created tab must too (EVE's "reset overview" reads them).
    fn user_with_tabs() -> Value {
        let tab = Value::Dict(vec![
            (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
            (Value::Bytes(b"color".to_vec()), Value::None),
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::Tuple(vec![ts(), Value::List(vec![Value::List(vec![Value::Int(0)])])])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)])
    }

    fn tab_name(v: &Value, idx: i64) -> String {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let td = dict_inner(tabs).unwrap();
        let (_, tab) = td.iter().find(|(k, _)| as_int(k) == Some(idx)).unwrap();
        let Value::Dict(fields) = tab else { panic!() };
        fields.iter().find_map(|(k, val)| match (k, val) {
            (Value::Str(s), Value::Str(name)) if s == "name" => Some(name.clone()),
            (Value::Str(s), Value::StrUcs2(name)) if s == "name" => Some(name.clone()),
            _ => None,
        }).unwrap()
    }

    fn tab_has_key(v: &Value, idx: i64, key: &[u8]) -> bool {
        let Value::Dict(root) = v else { return false };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { return false };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Some(td) = dict_inner(tabs) else { return false };
        let Some((_, tab)) = td.iter().find(|(k, _)| as_int(k) == Some(idx)) else { return false };
        let Value::Dict(fields) = tab else { return false };
        fields.iter().any(|(k, _)| is_b(k, key))
    }

    fn window_indices(v: &Value, window: usize) -> Vec<i64> {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, g) = ovd.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let outer = list_inner(g).unwrap();
        let inner = list_inner(&outer[window]).unwrap();
        inner.iter().filter_map(as_int).collect()
    }

    #[test]
    fn rename_sets_the_name_field() {
        let mut v = user_with_tabs();
        rename_tab(&mut v, 0, "Combat").unwrap();
        assert_eq!(tab_name(&v, 0), "Combat");
    }

    #[test]
    fn rename_unknown_tab_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(rename_tab(&mut v, 9, "X"), Err(OverviewTabError::UnknownTab { index: 9 })));
    }

    #[test]
    fn create_allocates_next_index_and_joins_the_window() {
        let mut v = user_with_tabs(); // has tab 0 in window 0
        let idx = create_tab(&mut v, 0, "Mining", Some(0)).unwrap();
        assert_eq!(idx, 1, "next free index after 0");
        assert_eq!(tab_name(&v, 1), "Mining");
        assert_eq!(window_indices(&v, 0), vec![0, 1], "appended to window 0's strip");
        // Regression: a created tab must clone the sibling's bracket + color,
        // else EVE's "reset all overview settings" throws on the malformed tab.
        assert!(tab_has_key(&v, 1, b"bracket"), "created tab clones the sibling's bracket");
        assert!(tab_has_key(&v, 1, b"color"), "created tab clones the sibling's color");
    }

    #[test]
    fn create_into_missing_window_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(create_tab(&mut v, 5, "X", Some(0)), Err(OverviewTabError::UnknownWindow { index: 5 })));
    }

    #[test]
    fn create_in_a_windowless_account_adds_the_tab_without_a_window_mapping() {
        // An overview with tabs but NO tabsByWindowInstanceID (fresh / post-reset).
        let tab = Value::Dict(vec![
            (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
            (Value::Bytes(b"color".to_vec()), Value::None),
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);

        // window_idx 0 is a sentinel here (there are no windows to name).
        let idx = create_tab(&mut v, 0, "Mining", Some(0)).unwrap();
        assert_eq!(idx, 1);
        assert!(tab_has_key(&v, 1, b"bracket"), "the new tab still clones bracket");
        // No window mapping is fabricated — EVE distributes tabs by default, and a
        // partial/wrong mapping would hide the whole overview.
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        assert!(!ovd.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")),
            "no tabsByWindowInstanceID is fabricated for a windowless account");
    }

    #[test]
    fn create_with_no_sibling_still_carries_bracket_and_color() {
        // Empty tabsettings_new -> no sibling to clone -> the fallback tab.
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![ts(), Value::Dict(vec![])])),
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);
        let idx = create_tab(&mut v, 0, "First", None).unwrap();
        assert_eq!(idx, 0);
        assert!(tab_has_key(&v, 0, b"bracket"), "fallback tab carries bracket");
        assert!(tab_has_key(&v, 0, b"color"), "fallback tab carries color");
    }

    #[test]
    fn delete_on_a_windowless_account_does_not_fabricate_a_mapping() {
        let mk = |n: &str| Value::Dict(vec![
            (Value::Str("name".into()), Value::Str(n.to_string())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), mk("A")), (Value::Int(1), mk("B"))])])),
            // no tabsByWindowInstanceID
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);
        delete_tab(&mut v, 0).unwrap();
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        assert!(!ovd.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")),
            "delete must not fabricate a window mapping on a windowless account");
    }

    #[test]
    fn delete_removes_tab_and_purges_window_strips() {
        let mut v = user_with_tabs();
        create_tab(&mut v, 0, "Mining", Some(0)).unwrap(); // now tabs 0,1 in window 0
        delete_tab(&mut v, 0).unwrap();
        assert_eq!(window_indices(&v, 0), vec![1], "0 purged from the strip");
        assert!(matches!(rename_tab(&mut v, 0, "X"), Err(OverviewTabError::UnknownTab { index: 0 })),
            "tab 0 is gone from tabsettings_new");
    }

    #[test]
    fn delete_last_tab_is_refused() {
        let mut v = user_with_tabs(); // single tab 0
        assert!(matches!(delete_tab(&mut v, 0), Err(OverviewTabError::LastTab)));
    }

    #[test]
    fn reorder_replaces_the_window_strip() {
        let mut v = user_with_tabs();
        create_tab(&mut v, 0, "Mining", Some(0)).unwrap(); // window 0 = [0,1]
        reorder_tabs_in_window(&mut v, 0, &[1, 0]).unwrap();
        assert_eq!(window_indices(&v, 0), vec![1, 0]);
    }

    #[test]
    fn reorder_missing_window_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(reorder_tabs_in_window(&mut v, 3, &[0]), Err(OverviewTabError::UnknownWindow { index: 3 })));
    }

    fn user_two_windows() -> Value {
        let tab = |p: &str| Value::Dict(vec![
            (Value::Str("name".into()), Value::Str(p.to_string())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab("A")), (Value::Int(1), tab("B"))])])),
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::Tuple(vec![ts(), Value::List(vec![
                 Value::List(vec![Value::Int(0)]), // window 0 = [0]
                 Value::List(vec![Value::Int(1)]), // window 1 = [1]
             ])])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)])
    }

    #[test]
    fn move_relocates_tab_between_windows() {
        let mut v = user_two_windows();
        move_tab(&mut v, 0, 0, 1, 0).unwrap();
        assert_eq!(window_indices(&v, 0), Vec::<i64>::new(), "removed from source");
        assert_eq!(window_indices(&v, 1), vec![0, 1], "inserted at pos 0 of target");
    }

    #[test]
    fn move_to_missing_window_errors() {
        let mut v = user_two_windows();
        assert!(matches!(move_tab(&mut v, 0, 0, 9, 0), Err(OverviewTabError::UnknownWindow { index: 9 })));
        assert_eq!(window_indices(&v, 0), vec![0], "source strip unchanged when destination is invalid");
    }

    #[test]
    fn move_of_a_tab_that_does_not_exist_errors() {
        let mut v = user_two_windows();
        assert!(matches!(move_tab(&mut v, 9, 0, 1, 0), Err(OverviewTabError::UnknownTab { index: 9 })));
        assert_eq!(window_indices(&v, 1), vec![1], "no phantom index in the destination strip");
    }

    #[test]
    fn add_window_appends_a_group_with_a_cloned_tab() {
        let mut v = user_with_tabs(); // one window [0]
        let widx = add_overview_window(&mut v, "Scan", Some(0)).unwrap();
        assert_eq!(widx, 1, "new window appended at index 1");
        let new_tabs = window_indices(&v, 1);
        assert_eq!(new_tabs.len(), 1, "new window seeded with exactly one tab");
        assert_eq!(tab_name(&v, new_tabs[0]), "Scan");
        // Seeded via create_tab -> carries bracket/color like every valid EVE tab.
        assert!(tab_has_key(&v, new_tabs[0], b"bracket"), "seeded tab clones bracket");
        assert!(tab_has_key(&v, new_tabs[0], b"color"), "seeded tab clones color");
        assert_eq!(window_indices(&v, 0), vec![0], "window 0 untouched");
    }

    #[test]
    fn add_window_on_a_windowless_account_is_refused() {
        // Overview with tabs but no tabsByWindowInstanceID: positional add can't
        // fabricate a base mapping without hiding the account's existing tabs.
        let tab = Value::Dict(vec![
            (Value::Bytes(b"bracket".to_vec()), Value::Bytes(b"_BracketFilterShowAll".to_vec())),
            (Value::Bytes(b"color".to_vec()), Value::None),
            (Value::Str("name".into()), Value::Str("Main".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        let overview = Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
        ]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), overview)]);
        assert!(matches!(add_overview_window(&mut v, "X", Some(0)), Err(OverviewTabError::NoWindowMapping)));
    }

    #[test]
    fn remove_last_window_reassigns_its_tabs_to_window_zero() {
        let mut v = user_two_windows(); // window 0 = [0], window 1 = [1]
        remove_overview_window(&mut v, 1).unwrap();
        assert_eq!(window_indices(&v, 0), vec![0, 1], "removed window's tab moved to window 0");
        // Only one window remains.
        let Value::Dict(root) = &v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, g) = ovd.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let outer = list_inner(g).unwrap();
        assert_eq!(outer.len(), 1, "one window left");
        assert_eq!(tab_name(&v, 1), "B", "no tab deleted");
    }

    #[test]
    fn remove_non_last_window_is_refused() {
        let mut v = user_two_windows();
        assert!(matches!(remove_overview_window(&mut v, 0), Err(OverviewTabError::NotLastWindow { index: 0 })));
    }

    #[test]
    fn remove_the_only_window_is_refused() {
        let mut v = user_with_tabs(); // one window
        assert!(matches!(remove_overview_window(&mut v, 0), Err(OverviewTabError::LastWindow)));
    }

    #[test]
    fn remove_unknown_window_errors() {
        let mut v = user_two_windows(); // two windows, indices 0 and 1
        assert!(matches!(remove_overview_window(&mut v, 2), Err(OverviewTabError::UnknownWindow { index: 2 })));
    }

    /// A char tree: `windows` (plain dict) -> two `(ts,dict)` subdicts, each with
    /// a primary `overview` entry (geometry tuple / bool flag), mirroring real files.
    fn char_with_primary_overview() -> Value {
        let geom = |x: i64| Value::Tuple(vec![
            Value::Int(x), Value::Int(100), Value::Int(400), Value::Int(300),
            Value::Int(2560), Value::Int(1440),
        ]);
        let sub = |entries: Vec<(Value, Value)>|
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(entries)]);
        let windows = Value::Dict(vec![
            (Value::Bytes(b"windowSizesAndPositions_1".to_vec()),
             sub(vec![(Value::Bytes(b"overview".to_vec()), geom(1000))])),
            (Value::Bytes(b"openWindows".to_vec()),
             sub(vec![(Value::Bytes(b"overview".to_vec()), Value::Bool(true))])),
        ]);
        Value::Dict(vec![(Value::Bytes(b"windows".to_vec()), windows)])
    }

    /// The window-id keys present in one `windows` subdict (tree already plain).
    fn win_keys(v: &Value, subdict: &[u8]) -> Vec<Vec<u8>> {
        let Value::Dict(root) = v else { panic!() };
        let (_, wins) = root.iter().find(|(k, _)| is_b(k, b"windows")).unwrap();
        let Value::Dict(subs) = wins else { panic!() };
        let (_, sv) = subs.iter().find(|(k, _)| is_b(k, subdict)).unwrap();
        let d = dict_inner(sv).unwrap();
        d.iter().filter_map(|(k, _)| if let Value::Bytes(b) = k { Some(b.clone()) } else { None }).collect()
    }

    /// The (x, y) of a window's geometry tuple in `windowSizesAndPositions_1`.
    fn geom_xy(v: &Value, key: &[u8]) -> (i64, i64) {
        let Value::Dict(root) = v else { panic!() };
        let (_, wins) = root.iter().find(|(k, _)| is_b(k, b"windows")).unwrap();
        let Value::Dict(subs) = wins else { panic!() };
        let (_, sv) = subs.iter().find(|(k, _)| is_b(k, b"windowSizesAndPositions_1")).unwrap();
        let d = dict_inner(sv).unwrap();
        let (_, g) = d.iter().find(|(k, _)| is_b(k, key)).unwrap();
        let Value::Tuple(items) = g else { panic!() };
        let Value::Int(x) = items[0] else { panic!() };
        let Value::Int(y) = items[1] else { panic!() };
        (x, y)
    }


    #[test]
    fn add_geometry_clones_primary_into_overview_n_with_offset() {
        let mut v = char_with_primary_overview();
        add_overview_window_geometry(&mut v, 1);
        assert!(win_keys(&v, b"windowSizesAndPositions_1").iter().any(|k| k == b"overview_1"),
            "overview_1 minted in the geometry subdict");
        assert!(win_keys(&v, b"openWindows").iter().any(|k| k == b"overview_1"),
            "overview_1 minted in the flags subdict too");
        assert_eq!(geom_xy(&v, b"overview"), (1000, 100), "primary unchanged");
        assert_eq!(geom_xy(&v, b"overview_1"), (1040, 140), "clone offset by (40, 40)");
    }

    #[test]
    fn add_geometry_is_idempotent() {
        let mut v = char_with_primary_overview();
        add_overview_window_geometry(&mut v, 1);
        add_overview_window_geometry(&mut v, 1);
        let count = win_keys(&v, b"windowSizesAndPositions_1")
            .iter().filter(|k| k.as_slice() == b"overview_1").count();
        assert_eq!(count, 1, "not double-added");
        assert_eq!(geom_xy(&v, b"overview_1"), (1040, 140), "not offset twice");
    }

    #[test]
    fn remove_geometry_drops_overview_n_everywhere() {
        let mut v = char_with_primary_overview();
        add_overview_window_geometry(&mut v, 1);
        remove_overview_window_geometry(&mut v, 1);
        assert!(!win_keys(&v, b"windowSizesAndPositions_1").iter().any(|k| k == b"overview_1"));
        assert!(!win_keys(&v, b"openWindows").iter().any(|k| k == b"overview_1"));
        assert!(win_keys(&v, b"windowSizesAndPositions_1").iter().any(|k| k == b"overview"),
            "primary untouched");
    }

    fn tab_preset(v: &Value, idx: i64) -> String {
        let Value::Dict(root) = v else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(ovd) = ov else { panic!() };
        let (_, tabs) = ovd.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let td = dict_inner(tabs).unwrap();
        let (_, tab) = td.iter().find(|(k, _)| as_int(k) == Some(idx)).unwrap();
        let Value::Dict(fields) = tab else { panic!() };
        let (_, val) = fields.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        match val { Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(), _ => panic!() }
    }

    #[test]
    fn set_tab_preset_changes_the_field() {
        let mut v = user_with_tabs();
        set_tab_preset(&mut v, 0, "combat").unwrap();
        assert_eq!(tab_preset(&v, 0), "combat");
    }

    #[test]
    fn the_windowless_message_does_not_read_as_damage() {
        let msg = OverviewTabError::NoWindowMapping.to_string();
        // The state is one EVE's own importer produces, so the wording has to
        // describe a configuration, never a fault. "no ... to add to" read as a
        // missing piece of the file.
        assert!(msg.contains("per-window"), "message should name the feature: {msg}");
        for bad in ["no window layout", "missing", "damaged", "corrupt", "invalid"] {
            assert!(!msg.to_lowercase().contains(bad), "message still reads as damage ({bad}): {msg}");
        }
    }

    #[test]
    fn set_tab_preset_unknown_tab_errors() {
        let mut v = user_with_tabs();
        assert!(matches!(
            set_tab_preset(&mut v, 9, "combat"),
            Err(OverviewTabError::UnknownTab { index: 9 })
        ));
    }

    /// An overview with tabs but no window mapping — the state EVE's own pack
    /// importer leaves behind (verified 2026-07-28).
    fn windowless_root() -> Value {
        let tab = Value::Dict(vec![
            (Value::Str("name".into()), Value::StrUcs2("Default".into())),
            (Value::Bytes(b"overview".to_vec()), Value::Bytes(b"P".to_vec())),
        ]);
        Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::Dict(vec![(Value::Int(0), tab)]),
            ])),
        ]))])
    }

    fn has_mapping(v: &Value) -> bool {
        let Value::Dict(top) = v else { return false };
        let Some((_, ov)) = top.iter().find(|(k, _)| is_b(k, b"overview")) else { return false };
        let Value::Dict(entries) = ov else { return false };
        entries.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID"))
    }

    #[test]
    fn reorder_on_a_windowless_account_refuses_without_fabricating() {
        let mut v = windowless_root();
        assert!(matches!(
            reorder_tabs_in_window(&mut v, 0, &[0]),
            Err(OverviewTabError::NoWindowMapping),
        ));
        assert!(!has_mapping(&v), "a refused reorder must not create the mapping");
    }

    #[test]
    fn move_on_a_windowless_account_refuses_without_fabricating() {
        let mut v = windowless_root();
        assert!(matches!(
            move_tab(&mut v, 0, 0, 0, 0),
            Err(OverviewTabError::NoWindowMapping),
        ));
        assert!(!has_mapping(&v), "a refused move must not create the mapping");
    }

    /// The mapping's single inner list, as plain ints.
    fn mapped_tabs(v: &Value) -> Vec<i64> {
        let Value::Dict(top) = v else { panic!() };
        let (_, ov) = top.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        let (_, wv) = entries.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let groups = list_inner(wv).unwrap();
        assert_eq!(groups.len(), 1, "exactly one window is created");
        list_inner(&groups[0]).unwrap().iter().filter_map(as_int).collect()
    }

    #[test]
    fn create_mapping_lists_every_tab_in_one_window() {
        let mut v = windowless_root();
        // Give it a second and third tab so "every tab, in index order" has
        // something to prove — a one-tab account cannot distinguish the cases.
        {
            let ov = overview_mut(&mut v).unwrap();
            let tabs = tabs_mut(ov);
            let clone = tabs[0].1.clone();
            tabs.push((Value::Int(5), clone.clone()));
            tabs.push((Value::Int(2), clone));
        }
        assert_eq!(create_window_mapping(&mut v).unwrap(), 3);
        // Ascending index order, and NOT dict order (0, 5, 2 as inserted).
        assert_eq!(mapped_tabs(&v), vec![0, 2, 5]);
    }

    #[test]
    fn create_mapping_refuses_when_one_already_exists() {
        let mut v = windowless_root();
        create_window_mapping(&mut v).unwrap();
        assert!(matches!(
            create_window_mapping(&mut v),
            Err(OverviewTabError::WindowMappingExists),
        ));
    }

    #[test]
    fn create_mapping_refuses_an_overview_with_no_tabs() {
        // A mapping whose only window lists no tabs would hide everything, and a
        // zero-tab overview is a state that exists in the wild.
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::Dict(Vec::new()),
            ])),
        ]))]);
        assert!(matches!(
            create_window_mapping(&mut v),
            Err(OverviewTabError::NoTabsToMap),
        ));
        assert!(!has_mapping(&v), "a refused create must not leave a partial mapping");
    }

    #[test]
    fn create_mapping_refuses_a_tab_it_cannot_name() {
        // Completeness is the safety property, so a tab key `as_int` cannot read
        // must refuse rather than be quietly skipped: a mapping that omits a tab
        // hides that tab in game while the editor still lists it, which is
        // exactly the failure this whole design exists to prevent.
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::StrUcs2("A".into()))]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Tuple(vec![
                Value::Long(vec![0u8; 8]),
                Value::Dict(vec![
                    (Value::Int(0), tab.clone()),
                    // Not an Int: silently dropped before this guard existed.
                    (Value::Long(vec![1u8; 8]), tab),
                ]),
            ])),
        ]))]);
        assert!(matches!(
            create_window_mapping(&mut v),
            Err(OverviewTabError::NoTabsToMap),
        ));
        assert!(!has_mapping(&v), "a refused create must not leave a partial mapping");
    }

    #[test]
    fn create_mapping_on_an_overview_with_no_tab_key_mints_nothing() {
        // `tabs_mut` CREATES `tabsettings_new` when absent, so reading through it
        // would leave an empty tab dict behind on a refused create — the same
        // "a refused edit must not touch the file" rule the reorder/move guards
        // exist for.
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(Vec::new()))]);
        assert!(matches!(
            create_window_mapping(&mut v),
            Err(OverviewTabError::NoTabsToMap),
        ));
        let Value::Dict(top) = &v else { panic!() };
        let (_, ov) = top.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        assert!(entries.is_empty(), "a refused create must not mint a tab dict: {entries:?}");
    }

    #[test]
    fn create_mapping_leaves_a_tree_that_still_encodes() {
        let mut v = windowless_root();
        create_window_mapping(&mut v).unwrap();
        let bytes = blue_marshal::encode(&v).expect("edited tree still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v);
    }

    #[test]
    fn add_window_works_once_a_mapping_exists() {
        // The whole point of the opt-in: add_overview_window refused before, and
        // must succeed afterwards.
        let mut v = windowless_root();
        assert!(matches!(add_overview_window(&mut v, "Second", None), Err(OverviewTabError::NoWindowMapping)));
        create_window_mapping(&mut v).unwrap();
        assert_eq!(add_overview_window(&mut v, "Second", None).unwrap(), 1);
    }

    /// The value stored under one `overview` container key, tree already plain.
    fn container_slot<'a>(v: &'a Value, key: &[u8]) -> &'a Value {
        let Value::Dict(top) = v else { panic!() };
        let (_, ov) = top.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        let (_, slot) = entries.iter().find(|(k, _)| is_b(k, key)).unwrap();
        slot
    }

    /// A bare payload is not a shape the client writes — 0 of 4,187 container
    /// keys across five untouched account files. One can only be there because
    /// an older build of this editor stripped the wrapper, so an edit that
    /// passes through must restore it rather than perpetuate it.
    #[test]
    fn editing_tabs_rewraps_a_bare_tabsettings() {
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::StrUcs2("A".into()))]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            // Deliberately bare, as an older build would have left it.
            (Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(Value::Int(0), tab)])),
        ]))]);
        rename_tab(&mut v, 0, "B").unwrap();

        let slot = container_slot(&v, b"tabsettings_new");
        let Value::Tuple(items) = slot else {
            panic!("a bare tabsettings_new must come back wrapped, got {slot:?}");
        };
        assert!(matches!(items[0], Value::Long(_)), "the wrapper leads with a timestamp");
        assert_eq!(tab_name(&v, 0), "B", "the edit itself still landed");
    }

    /// The same for the window-groups writer.
    #[test]
    fn editing_window_groups_rewraps_a_bare_mapping() {
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::StrUcs2("A".into()))]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
            // Deliberately bare.
            (Value::Bytes(b"tabsByWindowInstanceID".to_vec()),
             Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]))]);
        reorder_tabs_in_window(&mut v, 0, &[0]).unwrap();

        let slot = container_slot(&v, b"tabsByWindowInstanceID");
        let Value::Tuple(items) = slot else {
            panic!("a bare tabsByWindowInstanceID must come back wrapped, got {slot:?}");
        };
        assert!(matches!(items[0], Value::Long(_)), "the wrapper leads with a timestamp");
        assert_eq!(window_indices(&v, 0), vec![0], "the reorder itself still landed");
    }

    /// An EXISTING wrapper's own timestamp must survive — the repair is for a
    /// missing wrapper, not an excuse to reset a real one to zero.
    #[test]
    fn rewrapping_never_resets_an_existing_timestamp() {
        let stamp = Value::Long(vec![7u8; 8]);
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::StrUcs2("A".into()))]);
        let mut v = Value::Dict(vec![(Value::Bytes(b"overview".to_vec()), Value::Dict(vec![
            (Value::Bytes(b"tabsettings_new".to_vec()),
             Value::Tuple(vec![stamp.clone(), Value::Dict(vec![(Value::Int(0), tab)])])),
        ]))]);
        rename_tab(&mut v, 0, "B").unwrap();

        let slot = container_slot(&v, b"tabsettings_new");
        let Value::Tuple(items) = slot else { panic!() };
        assert_eq!(items[0], stamp, "an existing timestamp must not be reset to zero");
    }
}
