//! EVE overview *pack* import/export: the YAML file EVE's own Overview
//! Settings → Misc → Import/Export writes, and the format community packs are
//! published in.
//!
//! A pack encodes dicts as SEQUENCES OF TWO-ELEMENT `[key, value]` SEQUENCES
//! (python's `yaml.dump` of a list of tuples), so the only real YAML mapping in
//! the file is the top level. That is why `Node` has no map variant: a pack
//! "dict" is just `Node::Seq` of two-element `Node::Seq`s, and `pairs()` reads
//! it. All pack-format knowledge lives in this module; the rest of the crate
//! keeps speaking the marshal vocabulary.

use serde::Serialize;
use yaml_rust2::{Yaml, YamlLoader};

use blue_marshal::Value;

use crate::overview_states::OVERVIEW_BOOLS;
use crate::overview_tabs::{is_b, overview_mut};
use crate::treewalk::{collect_shared, effective, inline_all, Entries, SharedTable};

/// A YAML scalar or sequence. No map variant — see the module note.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Node>),
}

/// The section names a pack may carry, in the order the emitter writes them
/// (alphabetical, matching what real packs look like).
const SECTIONS: [&str; 13] = [
    "backgroundOrder",
    "backgroundStates",
    "columnOrder",
    "flagOrder",
    "flagStates",
    "overviewColumns",
    "presets",
    "shipLabelOrder",
    "shipLabels",
    "stateBlinks",
    "stateColorsNameList",
    "tabSetup",
    "userSettings",
];

/// One pack. EVERY section is optional: a pack carrying only `presets` is a
/// valid "preset pack" and must leave every other part of the account alone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pack {
    pub sections: Vec<(String, Node)>,
    /// Section names in the file that this build does not recognise. Reported
    /// to the user, never applied.
    pub ignored: Vec<String>,
}

impl Pack {
    pub fn get(&self, name: &str) -> Option<&Node> {
        self.sections.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    pub fn set(&mut self, name: &str, node: Node) {
        match self.sections.iter_mut().find(|(k, _)| k == name) {
            Some((_, slot)) => *slot = node,
            None => self.sections.push((name.to_string(), node)),
        }
    }
}

/// Struct variants only: an internally-tagged serde enum cannot serialize a
/// newtype variant holding a `String`. Mirrors `OverviewTabError`, `Display`
/// included, so `pack_err` in the app layer can lift the `code` tag out of the
/// serialization and the message out of `Display`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PackError {
    /// The file is not valid YAML.
    Yaml { message: String },
    /// Valid YAML, but the document is not a mapping (e.g. a bare list).
    NotAMapping,
    /// A mapping with no section this build recognises — the user picked the
    /// wrong file. Reported rather than silently applying nothing.
    NotAPack,
    /// A recognised section carrying the wrong YAML shape (a mapping where a
    /// list belongs). Distinct from `NotAPack`, which claims the file holds no
    /// pack sections at all — false, and confusing, for a real pack with one
    /// malformed section.
    BadSection { name: String },
    /// The document has no `overview` container to write into.
    NoOverview,
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::Yaml { message } => write!(f, "This file is not valid YAML: {message}"),
            PackError::NotAMapping => write!(f, "This file is not an overview pack."),
            PackError::NotAPack => write!(f, "This YAML file contains no overview pack sections."),
            PackError::BadSection { name } => {
                write!(f, "This pack's '{name}' section is not a list, so it cannot be applied.")
            }
            PackError::NoOverview => write!(f, "This file has no overview settings."),
        }
    }
}

/// A published pack drops the `2` the file's state-list keys carry. Accept both
/// spellings on read and normalise to the published one.
fn canonical_section(name: &str) -> Option<&'static str> {
    let stripped = match name {
        "backgroundStates2" => "backgroundStates",
        "backgroundOrder2" => "backgroundOrder",
        "flagStates2" => "flagStates",
        "flagOrder2" => "flagOrder",
        other => other,
    };
    SECTIONS.iter().find(|s| **s == stripped).copied()
}

/// Parse a pack. Tolerates a leading UTF-8 BOM (real packs carry one) — the
/// scanner treats it as stream-start whitespace, so no stripping happens here;
/// `strips_a_leading_bom` is the guard on that behaviour surviving a parser swap.
pub fn parse_pack(text: &str) -> Result<Pack, PackError> {
    let docs = YamlLoader::load_from_str(text)
        .map_err(|e| PackError::Yaml { message: e.to_string() })?;
    let Some(Yaml::Hash(top)) = docs.into_iter().next() else {
        return Err(PackError::NotAMapping);
    };

    let mut pack = Pack::default();
    for (k, v) in top {
        let Yaml::String(name) = k else { continue };
        match canonical_section(&name) {
            Some(section) => pack.sections.push((section.to_string(), node_from_yaml(&v))),
            None => pack.ignored.push(name),
        }
    }
    if pack.sections.is_empty() {
        return Err(PackError::NotAPack);
    }
    Ok(pack)
}

fn node_from_yaml(y: &Yaml) -> Node {
    match y {
        Yaml::Null | Yaml::BadValue => Node::Null,
        Yaml::Boolean(b) => Node::Bool(*b),
        Yaml::Integer(i) => Node::Int(*i),
        Yaml::Real(s) => s.parse::<f64>().map(Node::Float).unwrap_or_else(|_| Node::Str(s.clone())),
        Yaml::String(s) => Node::Str(s.clone()),
        Yaml::Array(items) => Node::Seq(items.iter().map(node_from_yaml).collect()),
        // A pack never nests a real mapping below the top level; if some future
        // vintage does, keep the pairs rather than dropping data.
        Yaml::Hash(h) => Node::Seq(
            h.iter().map(|(k, v)| Node::Seq(vec![node_from_yaml(k), node_from_yaml(v)])).collect(),
        ),
        Yaml::Alias(_) => Node::Null,
    }
}

/// Read a "dict" node: a sequence of two-element sequences. Entries of any
/// other shape are skipped.
pub fn pairs(n: &Node) -> Vec<(&Node, &Node)> {
    let Node::Seq(items) = n else { return Vec::new() };
    items
        .iter()
        .filter_map(|it| match it {
            Node::Seq(kv) if kv.len() == 2 => Some((&kv[0], &kv[1])),
            _ => None,
        })
        .collect()
}

pub fn ints(n: &Node) -> Vec<i64> {
    let Node::Seq(items) = n else { return Vec::new() };
    items.iter().filter_map(|i| match i { Node::Int(v) => Some(*v), _ => None }).collect()
}

pub fn strs(n: &Node) -> Vec<String> {
    let Node::Seq(items) = n else { return Vec::new() };
    items.iter().filter_map(|i| match i { Node::Str(s) => Some(s.clone()), _ => None }).collect()
}

pub fn as_str(n: &Node) -> Option<&str> {
    match n { Node::Str(s) => Some(s.as_str()), _ => None }
}

/// Render a pack as YAML EVE can import.
///
/// Style note: nested sequences are written on their OWN lines (`-` alone, then
/// the nested block indented) rather than the `- - -` run-in style python's
/// dumper produces. Both are the same YAML; matching CCP's dumper byte for byte
/// is an explicit non-goal, and the simple form is a third of the code.
pub fn emit_pack(pack: &Pack) -> String {
    let mut out = String::new();
    for name in SECTIONS {
        let Some(node) = pack.get(name) else { continue };
        out.push_str(name);
        out.push(':');
        match node {
            Node::Seq(items) if items.is_empty() => out.push_str(" []\n"),
            Node::Seq(items) => {
                out.push('\n');
                write_seq(&mut out, items, 0);
            }
            scalar => {
                out.push(' ');
                out.push_str(&write_scalar(scalar));
                out.push('\n');
            }
        }
    }
    out
}

fn write_seq(out: &mut String, items: &[Node], indent: usize) {
    for item in items {
        for _ in 0..indent { out.push(' '); }
        match item {
            Node::Seq(inner) if inner.is_empty() => out.push_str("- []\n"),
            Node::Seq(inner) => {
                out.push_str("-\n");
                write_seq(out, inner, indent + 2);
            }
            scalar => {
                out.push_str("- ");
                out.push_str(&write_scalar(scalar));
                out.push('\n');
            }
        }
    }
}

/// Quote every string: single-quoted (doubling `'`) normally, double-quoted with
/// `\n` escapes when the value contains a newline, since a single-quoted YAML
/// scalar folds line breaks instead of preserving them.
fn write_scalar(n: &Node) -> String {
    match n {
        Node::Null => "null".to_string(),
        Node::Bool(b) => b.to_string(),
        Node::Int(i) => i.to_string(),
        Node::Float(f) => format!("{f:?}"),
        Node::Str(s) if s.contains('\n') => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
            format!("\"{escaped}\"")
        }
        Node::Str(s) => format!("'{}'", s.replace('\'', "''")),
        Node::Seq(_) => unreachable!("write_seq handles sequences"),
    }
}

/// EVE's overview colour palette: the names a pack uses for a state's row
/// colour, and the RGBA the client writes for each.
///
/// HARVESTED FROM THE CORPUS, not from a client data file: an account that
/// imported a pack keeps the pack's `stateColorsNameList` under
/// `overview`→`restoreData`→`data` and the RGBA EVE derived from it under
/// `overview`→`stateColors`; joining them across the corpus yields this table
/// (`src/bin/pack_palette.rs`). A name absent here is skipped on import and a
/// colour absent here is omitted on export — never approximated, since a
/// near-miss would silently change the user's colours. Every name below
/// mapped to exactly one RGBA across all contributing corpus files — no
/// conflicts to resolve.
/// `black` was added on 2026-07-28 from a targeted live capture rather than
/// from the corpus join above, because the join could never have produced it:
/// no account here had ever imported a pack that names it, and the one
/// published pack that does (Z-S) puts it on `flag_48` — and **EVE's own
/// importer discards flag-surface colours outright**, so feeding Z-S through
/// the client left no trace of it at all.
///
/// The capture that worked: a probe pack (`tools/derive-packs.py`) moving the
/// name onto a background state, imported through EVE's own Overview Settings,
/// after which the client had written
/// `stateColors[("background", 66)] = (0.0, 0.0, 0.0, 1.0)`. That is the client
/// deriving the RGBA from the name, which is the same evidence the corpus join
/// provides. It independently matches the pixel-sampled `#000000` recorded for
/// state 66 in `overview-states.json`.
///
/// Still missing: `green` and `purple`. `overview-states.json`'s notes put the
/// full palette at eight names and give both as sampled hex (`#199919`,
/// `#9926e5`), but a sampled hex does not invert to an exact float — 25/255 is
/// 0.098…, consistent with both 0.098 and 0.1 — and `color_name` matches
/// exactly, by design. Harvest them the same way: probe pack, background state,
/// EVE's importer.
pub(crate) const PALETTE: [(&str, [f64; 4]); 6] = [
    ("black", [0.0, 0.0, 0.0, 1.0]),
    ("blue", [0.2, 0.5, 1.0, 1.0]),
    ("darkBlue", [0.0, 0.15, 0.6, 1.0]),
    ("orange", [1.0, 0.35, 0.0, 1.0]),
    ("red", [0.75, 0.0, 0.0, 1.0]),
    ("white", [0.7, 0.7, 0.7, 1.0]),
];

fn color_rgba(name: &str) -> Option<[f64; 4]> {
    PALETTE.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Exact match only. Two floats that differ in the last bit are not the same
/// colour name, and guessing the nearest one would rewrite a user's colours.
fn color_name(rgba: [f64; 4]) -> Option<&'static str> {
    PALETTE.iter().find(|(_, c)| *c == rgba).map(|(n, _)| *n)
}

/// `"background_16"` → `("background", 16)`. Both `stateColorsNameList` and
/// `stateBlinks` key their entries this way; the file keys them by a
/// `(surface, id)` tuple instead.
pub(crate) fn split_surface_key(s: &str) -> Option<(&str, i64)> {
    let (surface, id) = s.rsplit_once('_')?;
    if surface.is_empty() { return None }
    Some((surface, id.parse().ok()?))
}

pub(crate) fn join_surface_key(surface: &str, id: i64) -> String {
    format!("{surface}_{id}")
}

/// Pack section name → the `overview` container key holding it, for the sections
/// that are a plain list under a `(ts, list)` wrapper.
const LIST_SECTIONS: [(&str, &[u8]); 6] = [
    ("backgroundStates", b"backgroundStates2"),
    ("backgroundOrder", b"backgroundOrder2"),
    ("flagStates", b"flagStates2"),
    ("flagOrder", b"flagOrder2"),
    ("columnOrder", b"overviewColumnOrder"),
    ("overviewColumns", b"overviewColumns"),
];

/// What an import did, for the UI's summary line.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PackReport {
    /// Section names actually written.
    pub applied: Vec<String>,
    /// Anything ignored or skipped, in user-facing wording.
    pub warnings: Vec<String>,
}

/// Wrap a value in the `(timestamp, value)` shape EVE uses, minting a zero
/// timestamp like the rest of the crate. Reuses the existing wrapper when the
/// key is already present so an existing timestamp survives.
fn put(ov: &mut Entries, key: &[u8], value: Value) {
    match ov.iter_mut().find(|(k, _)| is_b(k, key)) {
        Some((_, slot)) => match slot {
            Value::Tuple(items) => {
                match items.iter_mut().find(|e| !matches!(e, Value::Long(_))) {
                    Some(inner) => *inner = value,
                    None => items.push(value),
                }
            }
            // A bare payload is not a shape the client writes — 0 of 4,187
            // container keys across five untouched account files. One can only
            // be here because an older build of this editor stripped the
            // wrapper, so restore it instead of perpetuating it.
            other => *other = Value::Tuple(vec![Value::Long(vec![0u8; 8]), value]),
        },
        None => ov.push((
            Value::Bytes(key.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), value]),
        )),
    }
}

/// Convert a ship-label field's pack value to the file's `Value` vocabulary.
/// Real labels carry `color` as a nested sequence of floats (the corpus has
/// files with a `List` of floats there, never a `Tuple`), so this must be
/// exhaustive over every `Node` variant — an earlier version fell through to
/// `_ => return None` inside a `filter_map`, which silently DROPPED the whole
/// field (not just left it at a default) for any label carrying a `color`.
fn label_value(n: &Node) -> Value {
    match n {
        Node::Null => Value::None,
        Node::Bool(x) => Value::Bool(*x),
        Node::Int(i) => Value::Int(*i),
        Node::Float(f) => Value::Float(*f),
        Node::Str(s) => Value::Bytes(s.clone().into_bytes()),
        Node::Seq(items) => Value::List(items.iter().map(label_value).collect()),
    }
}

/// Apply every section the pack defines. Sections it omits are left untouched.
///
/// ATOMICITY: every replacement value is built BEFORE the first mutation, so a
/// pack that fails conversion leaves the document exactly as it was. The only
/// error after that point is a missing `overview` container, which is checked
/// first.
pub fn apply_pack(v: &mut Value, pack: &Pack) -> Result<PackReport, PackError> {
    let mut report = PackReport::default();
    for name in &pack.ignored {
        report.warnings.push(format!("ignored unknown section '{name}'"));
    }

    // --- build phase (no mutation) ---
    // Every overview container key is stored `(timestamp, payload)`.
    let mut writes: Vec<(&[u8], Value)> = Vec::new();

    for (section, key) in LIST_SECTIONS {
        let Some(node) = pack.get(section) else { continue };
        // The two column lists hold column-name Bytes, the four state lists hold
        // Ints — but that is an element-type difference only. **All six are
        // stored `(timestamp, list)`**: every `core_user_*` file in the corpus
        // wraps both column keys, historical snapshots included, and so did this
        // account before its first pack import. This flag used to double as the
        // wrapper decision and stripped the wrapper off the two column keys on
        // every import. (`key` is a `&[u8]`, so compare against slices — a `b"…"`
        // pattern is a `&[u8; N]` and will not typecheck here.)
        let is_columns = key == b"overviewColumnOrder".as_slice()
            || key == b"overviewColumns".as_slice();
        let value = if is_columns {
            Value::List(strs(node).into_iter().map(|s| Value::Bytes(s.into_bytes())).collect())
        } else {
            let mut ids = ints(node);
            // Enabled lists are stored sorted and deduplicated; order lists keep
            // the pack's sequence (the slice-3 convention).
            if matches!(section, "backgroundStates" | "flagStates") {
                ids.sort_unstable();
                ids.dedup();
            }
            Value::List(ids.into_iter().map(Value::Int).collect())
        };
        writes.push((key, value));
        report.applied.push(section.to_string());
    }

    if let Some(node) = pack.get("stateColorsNameList") {
        let mut entries: Entries = Vec::new();
        for (k, val) in pairs(node) {
            let (Some(key), Some(name)) = (as_str(k), as_str(val)) else { continue };
            let Some((surface, id)) = split_surface_key(key) else { continue };
            let Some(rgba) = color_rgba(name) else {
                report.warnings.push(format!("unknown colour name '{name}' — left at EVE's default"));
                continue;
            };
            entries.push((
                Value::Tuple(vec![Value::Bytes(surface.as_bytes().to_vec()), Value::Int(id)]),
                Value::Tuple(vec![Value::Float(rgba[0]), Value::Float(rgba[1]), Value::Float(rgba[2]), Value::Float(rgba[3])]),
            ));
        }
        writes.push((b"stateColors", Value::Dict(entries)));
        report.applied.push("stateColorsNameList".to_string());
    }

    if let Some(node) = pack.get("stateBlinks") {
        let mut entries: Entries = Vec::new();
        for (k, val) in pairs(node) {
            let (Some(key), Node::Bool(on)) = (as_str(k), val) else { continue };
            let Some((surface, id)) = split_surface_key(key) else { continue };
            entries.push((
                Value::Tuple(vec![Value::Bytes(surface.as_bytes().to_vec()), Value::Int(id)]),
                Value::Bool(*on),
            ));
        }
        writes.push((b"stateBlinks", Value::Dict(entries)));
        report.applied.push("stateBlinks".to_string());
    }

    // shipLabels: rebuild the file's ordered list of label dicts from the pack's
    // order list plus its name-keyed bodies. Field values are Bytes, as the file
    // stores them; `state` stays an int.
    if let (Some(order), Some(labels)) = (pack.get("shipLabelOrder"), pack.get("shipLabels")) {
        let bodies = pairs(labels);
        let Node::Seq(order_items) = order else {
            return Err(PackError::BadSection { name: "shipLabelOrder".into() });
        };
        let mut list = Vec::new();
        for want in order_items {
            let Some((_, body)) = bodies.iter().find(|(k, _)| *k == want) else { continue };
            let fields: Entries = pairs(body)
                .into_iter()
                .filter_map(|(k, val)| {
                    let key = Value::Bytes(as_str(k)?.as_bytes().to_vec());
                    Some((key, label_value(val)))
                })
                .collect();
            list.push(Value::Dict(fields));
        }
        writes.push((b"shipLabels", Value::List(list)));
        report.applied.push("shipLabels".to_string());
    }

    if let Some(node) = pack.get("userSettings") {
        // A pack names these exactly as the file keys them, so `OVERVIEW_BOOLS`
        // is both the allow-list and the mapping. Packs also carry names with no
        // key on current files (`applyOnlyToShips`, an older single toggle that
        // became `applyToStructures`/`applyToOtherObjects` — confirmed absent
        // in-game 2026-07-27); those warn rather than being minted.
        for (k, val) in pairs(node) {
            let (Some(name), Node::Bool(on)) = (as_str(k), val) else { continue };
            match OVERVIEW_BOOLS.iter().find(|key| **key == name) {
                Some(key) => writes.push((key.as_bytes(), Value::Bool(*on))),
                None => report.warnings.push(format!("ignored unknown setting '{name}'")),
            }
        }
        report.applied.push("userSettings".to_string());
    }

    // Presets: name-keyed dict of the three int lists. Built here, written below.
    // An EMPTY section is read as "not defined", not "set to nothing": the rest
    // of this crate maintains a LastPreset/LastTab invariant (overview_tabs.rs /
    // overview_presets.rs) that a pack wiping every preset or tab would violate,
    // leaving an overview the client can't render. A blank state/colour list has
    // no such invariant and keeps meaning "set this to nothing".
    let presets_value = match pack.get("presets") {
        Some(node) if is_empty_section(node) => {
            report.warnings.push("ignored empty 'presets' section (would have left the account with no presets)".to_string());
            None
        }
        Some(node) => {
            let mut entries: Entries = Vec::new();
            for (name, body) in pairs(node) {
                let Some(name) = as_str(name) else { continue };
                let fields: Entries = pairs(body)
                    .into_iter()
                    .filter_map(|(k, val)| {
                        let key = Value::Bytes(as_str(k)?.as_bytes().to_vec());
                        Some((key, Value::List(ints(val).into_iter().map(Value::Int).collect())))
                    })
                    .collect();
                entries.push((Value::Bytes(name.as_bytes().to_vec()), Value::Dict(fields)));
            }
            Some(Value::Dict(entries))
        }
        None => None,
    };

    let tabs_to_apply = match pack.get("tabSetup") {
        Some(node) if is_empty_section(node) => {
            report.warnings.push("ignored empty 'tabSetup' section (would have left the account with no tabs)".to_string());
            None
        }
        Some(node) => Some(node),
        None => None,
    };

    // --- mutate phase ---
    inline_all(v);
    let ov = overview_mut(v).map_err(|_| PackError::NoOverview)?;
    for (key, value) in writes {
        put(ov, key, value);
    }

    if let Some(value) = presets_value {
        put(ov, b"overviewProfilePresets", value);
        report.applied.push("presets".to_string());
        // A stale unsaved working copy under a name the pack does not define
        // would resurrect a phantom preset, exactly as rename/delete guard
        // against in slice 2a.
        ov.retain(|(k, _)| !is_b(k, b"overviewProfilePresets_notSaved"));
    }

    if let Some(node) = tabs_to_apply {
        let emptied = apply_tabs(ov, node);
        report.applied.push("tabSetup".to_string());
        if emptied > 0 {
            report.warnings.push(format!(
                "{emptied} overview window(s) ended up with no tabs (the pack has fewer tabs than the account's window layout expects)"
            ));
        }
    }

    Ok(report)
}

/// An empty `presets`/`tabSetup` section (`presets: []` in the file) — see the
/// note at the `presets_value`/`tabs_to_apply` build above.
fn is_empty_section(node: &Node) -> bool {
    matches!(node, Node::Seq(items) if items.is_empty())
}

/// Replace the tab dict from a pack's `tabSetup`, then re-point the window
/// mapping. New tabs CLONE an existing tab so they keep the `bracket`/`color`
/// keys EVE's reset path reads; per-tab column overrides are dropped, because
/// pack columns are account-global.
///
/// Returns how many windows in the mapping end up with no tabs at all. A pack
/// with fewer tabs than the account previously had can leave a secondary
/// window empty — unavoidable, since a pack carries no window model — so the
/// caller reports it as a warning instead of hiding it.
fn apply_tabs(ov: &mut Entries, node: &Node) -> usize {
    use crate::overview_tabs::{as_int, dict_inner_mut, fallback_tab, list_inner_mut as window_list_mut, tabs_mut};

    let template = {
        let tabs = tabs_mut(ov);
        let mut lowest: Option<(i64, Value)> = None;
        for (k, val) in tabs.iter() {
            if let Some(idx) = as_int(k) {
                if lowest.as_ref().map_or(true, |(best, _)| idx < *best) {
                    lowest = Some((idx, val.clone()));
                }
            }
        }
        lowest.map(|(_, val)| val)
    };

    let mut fresh: Entries = Vec::new();
    let mut indices: Vec<i64> = Vec::new();
    for (idx, body) in pairs(node) {
        let Node::Int(idx) = idx else { continue };
        // Zero-tab account: no sibling to clone. Reuse overview_tabs::create_tab's
        // own no-sibling fallback rather than a bespoke one — a fallback missing
        // `bracket` produced a tab EVE's "reset all overview settings" throws on
        // when the pack's own tab body doesn't supply it either.
        let mut tab = template.clone().unwrap_or_else(fallback_tab);
        let Some(fields) = dict_inner_mut(&mut tab) else { continue };
        // Column overrides belong to the account, not the pack.
        fields.retain(|(k, _)| !is_b(k, b"tabColumns") && !is_b(k, b"tabColumnOrder"));
        for (k, val) in pairs(body) {
            let (Some(key), Some(text)) = (as_str(k), as_str(val)) else { continue };
            match key {
                "name" => {
                    fields.retain(|(k, _)| !matches!(k, Value::StrTable(52)) && !is_b(k, b"name") && !matches!(k, Value::Str(s) if s == "name"));
                    fields.push((Value::Str("name".into()), Value::StrUcs2(text.to_string())));
                }
                "bracket" | "overview" => {
                    let kb = key.as_bytes().to_vec();
                    fields.retain(|(k, _)| !is_b(k, &kb));
                    fields.push((Value::Bytes(kb), Value::Bytes(text.as_bytes().to_vec())));
                }
                _ => {}
            }
        }
        fresh.push((Value::Int(*idx), tab));
        indices.push(*idx);
    }

    let tabs = tabs_mut(ov);
    *tabs = fresh;

    // Re-point the window mapping, only if the account has one (never fabricate).
    let Some((_, groups_val)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) else { return 0 };
    let Some(groups) = window_list_mut(groups_val) else { return 0 };

    // Filter the SECONDARY windows first, keeping only indices the pack still
    // defines, and remember what they claimed. Window 0 (the "everything else"
    // window) then gets only what's left over — giving it every pack index
    // up front, as the old code did, guarantees overlap with whatever a
    // secondary window also kept.
    let mut claimed: Vec<i64> = Vec::new();
    for window in groups.iter_mut().skip(1) {
        let Some(list) = window_list_mut(window) else { continue };
        list.retain(|e| matches!(e, Value::Int(i) if indices.contains(i)));
        claimed.extend(list.iter().filter_map(as_int));
    }
    if let Some(list) = groups.first_mut().and_then(window_list_mut) {
        *list = indices.iter().filter(|i| !claimed.contains(i)).map(|i| Value::Int(*i)).collect();
    }

    let mut emptied = 0;
    for window in groups.iter_mut() {
        if window_list_mut(window).map_or(false, |l| l.is_empty()) {
            emptied += 1;
        }
    }
    emptied
}

fn shared_is_b<'a>(k: &'a Value, name: &[u8], sh: &SharedTable<'a>) -> bool {
    matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name)
}

fn overview_entries<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    let Value::Dict(root) = effective(v, sh) else { return None };
    let (_, ov) = root.iter().find(|(k, _)| shared_is_b(k, b"overview", sh))?;
    match effective(ov, sh) { Value::Dict(d) => Some(d), _ => None }
}

fn find<'a>(ov: &'a Entries, key: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    ov.iter().find(|(k, _)| shared_is_b(k, key, sh)).map(|(_, v)| v)
}

/// Unwrap a `(timestamp, x)` wrapper, resolving indirection at both hops.
fn unwrapped<'a>(v: &'a Value, sh: &SharedTable<'a>) -> &'a Value {
    match effective(v, sh) {
        Value::Tuple(items) => items
            .iter()
            .map(|e| effective(e, sh))
            .find(|e| matches!(e, Value::Dict(_) | Value::List(_) | Value::Bool(_)))
            .unwrap_or(effective(v, sh)),
        other => other,
    }
}

fn text<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<String> {
    match effective(v, sh) {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}

fn node_of<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Node {
    match effective(v, sh) {
        Value::None => Node::Null,
        Value::Bool(x) => Node::Bool(*x),
        Value::Int(i) => Node::Int(*i),
        Value::Float(f) => Node::Float(*f),
        Value::Bytes(b) => Node::Str(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Node::Str(s.clone()),
        Value::List(items) | Value::Tuple(items) => Node::Seq(items.iter().map(|e| node_of(e, sh)).collect()),
        Value::Dict(d) => Node::Seq(
            d.iter().map(|(k, val)| Node::Seq(vec![node_of(k, sh), node_of(val, sh)])).collect(),
        ),
        _ => Node::Null,
    }
}

fn pair(k: &str, v: Node) -> Node { Node::Seq(vec![Node::Str(k.to_string()), v]) }

/// Project the account's overview as a pack, plus warnings for anything the
/// pack format cannot express.
pub fn read_pack(v: &Value) -> (Pack, Vec<String>) {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    let mut pack = Pack::default();
    let mut warnings = Vec::new();
    let Some(ov) = overview_entries(v, &sh) else { return (pack, warnings) };

    for (section, key) in LIST_SECTIONS {
        let Some(raw) = find(ov, key, &sh) else { continue };
        let Value::List(items) = unwrapped(raw, &sh) else { continue };
        pack.set(section, Node::Seq(items.iter().map(|e| node_of(e, &sh)).collect()));
    }

    // presets: name -> {alwaysShownStates, filteredStates, groups} (sorted keys,
    // as published packs have them)
    if let Some(raw) = find(ov, b"overviewProfilePresets", &sh) {
        if let Value::Dict(d) = unwrapped(raw, &sh) {
            let mut out = Vec::new();
            for (k, body) in d {
                let Some(name) = text(k, &sh) else { continue };
                let Value::Dict(fields) = effective(body, &sh) else { continue };
                let mut kv: Vec<(String, Node)> = fields
                    .iter()
                    .filter_map(|(fk, fv)| Some((text(fk, &sh)?, node_of(fv, &sh))))
                    .collect();
                kv.sort_by(|a, b| a.0.cmp(&b.0));
                let body = Node::Seq(kv.into_iter().map(|(k, v)| pair(&k, v)).collect());
                out.push(Node::Seq(vec![Node::Str(name), body]));
            }
            out.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
            pack.set("presets", Node::Seq(out));
        }
    }

    // tabSetup: index -> {bracket, name, overview}. Per-tab column overrides are
    // deliberately NOT exported — pack columns are account-global.
    if let Some(raw) = find(ov, b"tabsettings_new", &sh).or_else(|| find(ov, b"tabsettings", &sh)) {
        if let Value::Dict(d) = unwrapped(raw, &sh) {
            let mut out = Vec::new();
            for (k, body) in d {
                let Value::Int(idx) = effective(k, &sh) else { continue };
                let Value::Dict(fields) = effective(body, &sh) else { continue };
                let mut kv: Vec<(String, Node)> = Vec::new();
                for name in ["bracket", "name", "overview"] {
                    if let Some((_, fv)) = fields.iter().find(|(fk, _)| {
                        text(fk, &sh).as_deref() == Some(name) || matches!(effective(fk, &sh), Value::StrTable(52) if name == "name")
                    }) {
                        kv.push((name.to_string(), Node::Str(text(fv, &sh).unwrap_or_default())));
                    }
                }
                let body = Node::Seq(kv.into_iter().map(|(k, v)| pair(&k, v)).collect());
                out.push(Node::Seq(vec![Node::Int(*idx), body]));
            }
            out.sort_by_key(|e| match e { Node::Seq(kv) => match kv[0] { Node::Int(i) => i, _ => 0 }, _ => 0 });
            pack.set("tabSetup", Node::Seq(out));
        }
    }

    // stateColors -> stateColorsNameList (palette names only)
    if let Some(raw) = find(ov, b"stateColors", &sh) {
        if let Value::Dict(d) = unwrapped(raw, &sh) {
            let mut out = Vec::new();
            let mut omitted = 0usize;
            for (k, val) in d {
                let Value::Tuple(kp) = effective(k, &sh) else { continue };
                let [surface, id] = kp.as_slice() else { continue };
                let (Some(surface), Value::Int(id)) = (text(surface, &sh), effective(id, &sh)) else { continue };
                let Value::Tuple(parts) = effective(val, &sh) else { continue };
                let comps: Vec<f64> = parts.iter().filter_map(|c| match effective(c, &sh) {
                    Value::Float(f) => Some(*f),
                    Value::Int(i) => Some(*i as f64),
                    _ => None,
                }).collect();
                let [r, g, bl, a] = comps.as_slice() else { continue };
                match color_name([*r, *g, *bl, *a]) {
                    Some(name) => out.push(Node::Seq(vec![
                        Node::Str(join_surface_key(&surface, *id)),
                        Node::Str(name.to_string()),
                    ])),
                    None => omitted += 1,
                }
            }
            out.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
            pack.set("stateColorsNameList", Node::Seq(out));
            if omitted > 0 {
                warnings.push(format!("{omitted} custom colour(s) had no pack name and were omitted"));
            }
        }
    }

    // stateBlinks: (surface, id) -> bool
    if let Some(raw) = find(ov, b"stateBlinks", &sh) {
        if let Value::Dict(d) = unwrapped(raw, &sh) {
            let mut out = Vec::new();
            for (k, val) in d {
                let Value::Tuple(kp) = effective(k, &sh) else { continue };
                let [surface, id] = kp.as_slice() else { continue };
                let (Some(surface), Value::Int(id)) = (text(surface, &sh), effective(id, &sh)) else { continue };
                let Value::Bool(on) = effective(val, &sh) else { continue };
                out.push(Node::Seq(vec![Node::Str(join_surface_key(&surface, *id)), Node::Bool(*on)]));
            }
            out.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
            pack.set("stateBlinks", Node::Seq(out));
        }
    }

    // shipLabels / shipLabelOrder: the file stores one ordered list of label
    // dicts; the pack splits it into an order list and a name-keyed list.
    if let Some(raw) = find(ov, b"shipLabels", &sh) {
        if let Value::List(items) = unwrapped(raw, &sh) {
            let mut order = Vec::new();
            let mut labels = Vec::new();
            for item in items {
                let Value::Dict(fields) = effective(item, &sh) else { continue };
                let mut kv: Vec<(String, Node)> = fields
                    .iter()
                    .filter_map(|(fk, fv)| Some((text(fk, &sh)?, node_of(fv, &sh))))
                    .collect();
                kv.sort_by(|a, b| a.0.cmp(&b.0));
                let name = kv.iter().find(|(k, _)| k == "type").map(|(_, v)| v.clone()).unwrap_or(Node::Null);
                order.push(name.clone());
                labels.push(Node::Seq(vec![name, Node::Seq(kv.into_iter().map(|(k, v)| pair(&k, v)).collect())]));
            }
            pack.set("shipLabelOrder", Node::Seq(order));
            pack.set("shipLabels", Node::Seq(labels));
        }
    }

    // userSettings
    let settings: Vec<Node> = OVERVIEW_BOOLS
        .iter()
        .filter_map(|key| {
            let raw = find(ov, key.as_bytes(), &sh)?;
            let Value::Bool(on) = unwrapped(raw, &sh) else { return None };
            Some(Node::Seq(vec![Node::Str((*key).to_string()), Node::Bool(*on)]))
        })
        .collect();
    if !settings.is_empty() {
        pack.set("userSettings", Node::Seq(settings));
    }

    // `emit_pack` always writes sections in `SECTIONS` (canonical alphabetical)
    // order regardless of `pack.sections`'s own order, and re-parsing that text
    // reproduces the same canonical order — so a freshly-read pack must already
    // be sorted that way, or comparing it against an emit-then-reparse copy
    // (`a_read_pack_emits_and_reparses`) would spuriously fail on order alone
    // despite carrying identical data.
    pack.sections.sort_by_key(|(name, _)| SECTIONS.iter().position(|s| s == name).unwrap_or(usize::MAX));

    (pack, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pack fixture in the real published shape: dicts encoded as sequences of
    /// two-element [key, value] sequences, unicode and EVE markup in names, a
    /// `''`-escaped quote, and a tab label containing a newline. Written by hand
    /// — NOT copied from a published pack — so no third-party licence enters the
    /// repo.
    const FIXTURE: &str = r#"backgroundOrder:
- 13
- 9
backgroundStates:
- 9
- 13
presets:
- - '✪ Friendly: Fleet'
  - - - alwaysShownStates
      - []
    - - filteredStates
      - - 21
        - 36
    - - groups
      - - 25
        - 26
- - 'Bob''s picks'
  - - - alwaysShownStates
      - []
    - - filteredStates
      - []
    - - groups
      - - 27
stateColorsNameList:
- - background_16
  - darkBlue
tabSetup:
- - 0
  - - - bracket
      - '✪ Friendly: Fleet'
    - - name
      - "line one\nline two"
    - - overview
      - 'Bob''s picks'
userSettings:
- - overviewBroadcastsToTop
  - true
"#;

    #[test]
    fn parses_every_section_of_the_fixture() {
        let pack = parse_pack(FIXTURE).unwrap();
        assert_eq!(ints(pack.get("backgroundStates").unwrap()), vec![9, 13]);
        assert_eq!(ints(pack.get("backgroundOrder").unwrap()), vec![13, 9]);

        let presets = pairs(pack.get("presets").unwrap());
        assert_eq!(presets.len(), 2);
        assert_eq!(as_str(presets[0].0), Some("✪ Friendly: Fleet"));
        let fields = pairs(presets[0].1);
        let groups = fields.iter().find(|(k, _)| as_str(k) == Some("groups")).unwrap().1;
        assert_eq!(ints(groups), vec![25, 26]);
        assert_eq!(as_str(presets[1].0), Some("Bob's picks"), "'' is an escaped quote");

        let tabs = pairs(pack.get("tabSetup").unwrap());
        assert_eq!(tabs[0].0, &Node::Int(0));
        let tab = pairs(tabs[0].1);
        let name = tab.iter().find(|(k, _)| as_str(k) == Some("name")).unwrap().1;
        assert_eq!(as_str(name), Some("line one\nline two"));

        let colors = pairs(pack.get("stateColorsNameList").unwrap());
        assert_eq!((as_str(colors[0].0), as_str(colors[0].1)), (Some("background_16"), Some("darkBlue")));

        let settings = pairs(pack.get("userSettings").unwrap());
        assert_eq!(settings[0].1, &Node::Bool(true));
    }

    #[test]
    fn strips_a_leading_bom() {
        let with_bom = format!("\u{feff}{FIXTURE}");
        assert!(parse_pack(&with_bom).is_ok());
    }

    #[test]
    fn rejects_a_yaml_file_that_is_not_a_pack() {
        let err = parse_pack("some: mapping\nother: 3\n").unwrap_err();
        assert!(matches!(err, PackError::NotAPack));
    }

    #[test]
    fn rejects_a_document_that_is_not_a_mapping() {
        let err = parse_pack("- just\n- a list\n").unwrap_err();
        assert!(matches!(err, PackError::NotAMapping));
    }

    #[test]
    fn rejects_malformed_yaml() {
        let err = parse_pack("presets:\n- - unclosed: [\n").unwrap_err();
        assert!(matches!(err, PackError::Yaml { .. }), "got {err:?}");
    }

    #[test]
    fn accepts_the_suffixed_state_list_spelling() {
        // Published packs drop the `2`; should a client vintage emit the file's
        // own spelling, it must land in the same section.
        let pack = parse_pack("backgroundStates2:\n- 9\nflagOrder2:\n- 13\n").unwrap();
        assert_eq!(ints(pack.get("backgroundStates").unwrap()), vec![9]);
        assert_eq!(ints(pack.get("flagOrder").unwrap()), vec![13]);
        assert!(pack.ignored.is_empty());
    }

    #[test]
    fn keeps_unrecognised_sections_out_of_the_pack() {
        let pack = parse_pack("presets: []\nsomeFutureSection:\n- 1\n").unwrap();
        assert!(pack.get("someFutureSection").is_none());
        assert_eq!(pack.ignored, vec!["someFutureSection".to_string()]);
    }

    #[test]
    fn emit_then_parse_round_trips_the_fixture() {
        let pack = parse_pack(FIXTURE).unwrap();
        let text = emit_pack(&pack);
        let again = parse_pack(&text).unwrap();
        assert_eq!(again.sections, pack.sections, "round trip changed the tree:\n{text}");
    }

    #[test]
    fn emits_no_bom_and_sections_in_order() {
        let pack = parse_pack(FIXTURE).unwrap();
        let text = emit_pack(&pack);
        assert!(!text.starts_with('\u{feff}'));
        let pos = |s: &str| text.find(&format!("\n{s}:")).or_else(|| text.strip_prefix(s).map(|_| 0));
        assert!(text.starts_with("backgroundOrder:"), "first section is alphabetical: {text}");
        assert!(pos("presets") < pos("tabSetup"));
    }

    #[test]
    fn quotes_scalars_that_need_it() {
        let mut pack = Pack::default();
        pack.set("presets", Node::Seq(vec![Node::Seq(vec![
            Node::Str("It's <b>bold</b>".into()),
            Node::Seq(vec![Node::Seq(vec![Node::Str("groups".into()), Node::Seq(vec![])])]),
        ])]));
        // Neither of these parses back to the same string unless quoted: unquoted,
        // a leading `-` reads as a nested sequence entry, and `: ` reads as a
        // nested mapping.
        pack.set("columnOrder", Node::Seq(vec![
            Node::Str("- leading dash".into()),
            Node::Str("key: value-shaped".into()),
        ]));
        let text = emit_pack(&pack);
        let again = parse_pack(&text).unwrap();
        let name = pairs(again.get("presets").unwrap())[0].0;
        assert_eq!(as_str(name), Some("It's <b>bold</b>"));
        assert!(text.contains("[]"), "an empty sequence emits as []: {text}");
        assert_eq!(
            strs(again.get("columnOrder").unwrap()),
            vec!["- leading dash".to_string(), "key: value-shaped".to_string()],
        );
    }

    #[test]
    fn round_trips_a_multiline_scalar() {
        let mut pack = Pack::default();
        pack.set("columnOrder", Node::Seq(vec![Node::Str("two\nlines".into())]));
        let again = parse_pack(&emit_pack(&pack)).unwrap();
        assert_eq!(strs(again.get("columnOrder").unwrap()), vec!["two\nlines".to_string()]);
    }

    #[test]
    fn round_trips_a_null_entry() {
        // Real published packs carry a literal `null` in shipLabelOrder (and a
        // `null` key in shipLabels); node_from_yaml maps blank/`~` YAML to
        // Node::Null, so re-exporting a parsed real pack hits this path.
        let mut pack = Pack::default();
        pack.set("shipLabelOrder", Node::Seq(vec![Node::Null, Node::Str("hull".into())]));
        let again = parse_pack(&emit_pack(&pack)).unwrap();
        assert_eq!(again.get("shipLabelOrder"), pack.get("shipLabelOrder"));
    }

    #[test]
    fn round_trips_an_integral_float() {
        // {f:?} (not {f}) is what keeps an integral float's `.0` so it does not
        // silently reparse as an int.
        let mut pack = Pack::default();
        pack.set("userSettings", Node::Seq(vec![Node::Seq(vec![
            Node::Str("someFloatSetting".into()),
            Node::Float(3.0),
        ])]));
        let again = parse_pack(&emit_pack(&pack)).unwrap();
        let settings = pairs(again.get("userSettings").unwrap());
        assert_eq!(settings[0].1, &Node::Float(3.0));
    }

    #[test]
    fn writes_a_bare_scalar_section_with_a_space_after_the_colon() {
        // Sections aren't always Seqs: a section whose value is a plain scalar
        // takes the `name: value` branch of emit_pack, not write_seq.
        let mut pack = Pack::default();
        pack.set("columnOrder", Node::Bool(true));
        let text = emit_pack(&pack);
        assert!(text.contains("columnOrder: true\n"), "missing space after colon: {text}");
        let again = parse_pack(&text).unwrap();
        assert_eq!(again.get("columnOrder"), Some(&Node::Bool(true)));
    }

    #[test]
    fn palette_maps_both_directions() {
        assert_eq!(color_rgba("darkBlue"), Some([0.0, 0.15, 0.6, 1.0]));
        assert_eq!(color_rgba("blue"), Some([0.2, 0.5, 1.0, 1.0]));
        assert_eq!(color_rgba("red"), Some([0.75, 0.0, 0.0, 1.0]));
        assert_eq!(color_name([0.0, 0.15, 0.6, 1.0]), Some("darkBlue"));
        assert_eq!(color_rgba("chartreuse"), None);
        assert_eq!(color_name([0.123, 0.0, 0.0, 1.0]), None, "no near-miss matching");
    }

    #[test]
    fn palette_has_no_duplicate_colours() {
        for (i, (_, a)) in PALETTE.iter().enumerate() {
            for (_, b) in PALETTE.iter().skip(i + 1) {
                assert_ne!(a, b, "two names share an RGBA, so color_name is ambiguous");
            }
        }
    }

    #[test]
    fn splits_and_joins_surface_keys() {
        assert_eq!(split_surface_key("background_16"), Some(("background", 16)));
        assert_eq!(split_surface_key("flag_9"), Some(("flag", 9)));
        assert_eq!(split_surface_key("background"), None);
        assert_eq!(split_surface_key("background_x"), None);
        // Every real surface name is a single word, so this case never occurs in
        // the corpus; it exists only to pin "split on the LAST underscore" as the
        // contract, since a single-underscore case can't distinguish that from
        // splitting on the first.
        assert_eq!(split_surface_key("some_thing_5"), Some(("some_thing", 5)));
        assert_eq!(join_surface_key("flag", 9), "flag_9".to_string());
    }

    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    // A distinguishable non-zero seed (not all-zero): Task 5's write path mints
    // a zero `Long` timestamp for an ABSENT key while preserving an EXISTING
    // key's own timestamp untouched, and an all-zero fixture cannot tell those
    // two behaviours apart.
    fn ts() -> Value { Value::Long(vec![7, 0, 0, 0, 0, 0, 0, 0]) }
    fn ints_v(xs: &[i64]) -> Value { Value::List(xs.iter().map(|n| Value::Int(*n)).collect()) }

    /// A minimal but realistic `core_user` tree: overview container with one
    /// preset, one tab, columns, state lists, one colour, one blink and one bool.
    fn user_doc() -> Value {
        let preset = Value::Dict(vec![
            (b("groups"), ints_v(&[25, 26])),
            (b("filteredStates"), ints_v(&[21])),
            (b("alwaysShownStates"), ints_v(&[])),
        ]);
        let tab = Value::Dict(vec![
            (b("color"), Value::None),
            (b("bracket"), b("Brackets")),
            (b("name"), Value::StrUcs2("Fleet".into())),
            (b("overview"), b("Friendly")),
        ]);
        Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("overviewProfilePresets"), Value::Tuple(vec![ts(), Value::Dict(vec![(b("Friendly"), preset)])])),
            (b("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
            (b("overviewColumnOrder"), Value::List(vec![b("ICON"), b("NAME")])),
            (b("overviewColumns"), Value::List(vec![b("NAME")])),
            (b("backgroundStates2"), Value::Tuple(vec![ts(), ints_v(&[9, 13])])),
            (b("backgroundOrder2"), Value::Tuple(vec![ts(), ints_v(&[13, 9])])),
            (b("flagStates2"), Value::Tuple(vec![ts(), ints_v(&[9])])),
            (b("flagOrder2"), Value::Tuple(vec![ts(), ints_v(&[9, 13])])),
            (b("stateColors"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Tuple(vec![b("background"), Value::Int(16)]),
                 Value::Tuple(vec![Value::Float(0.0), Value::Float(0.15), Value::Float(0.6), Value::Float(1.0)])),
                (Value::Tuple(vec![b("background"), Value::Int(18)]),
                 Value::Tuple(vec![Value::Float(0.42), Value::Float(0.42), Value::Float(0.42), Value::Float(1.0)])),
            ])])),
            (b("stateBlinks"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Tuple(vec![b("flag"), Value::Int(9)]), Value::Bool(true)),
            ])])),
            (b("overviewBroadcastsToTop"), Value::Tuple(vec![ts(), Value::Bool(true)])),
        ]))])
    }

    #[test]
    fn reads_every_section_from_a_file() {
        let (pack, warnings) = read_pack(&user_doc());

        assert_eq!(ints(pack.get("backgroundStates").unwrap()), vec![9, 13]);
        assert_eq!(ints(pack.get("backgroundOrder").unwrap()), vec![13, 9]);
        assert_eq!(ints(pack.get("flagStates").unwrap()), vec![9]);
        assert_eq!(strs(pack.get("columnOrder").unwrap()), vec!["ICON".to_string(), "NAME".to_string()]);
        assert_eq!(strs(pack.get("overviewColumns").unwrap()), vec!["NAME".to_string()]);

        let presets = pairs(pack.get("presets").unwrap());
        assert_eq!(as_str(presets[0].0), Some("Friendly"));
        let fields = pairs(presets[0].1);
        assert_eq!(as_str(fields[0].0), Some("alwaysShownStates"), "preset fields are sorted");
        let groups = fields.iter().find(|(k, _)| as_str(k) == Some("groups")).unwrap().1;
        assert_eq!(ints(groups), vec![25, 26]);

        let tabs = pairs(pack.get("tabSetup").unwrap());
        assert_eq!(tabs[0].0, &Node::Int(0));
        let tab = pairs(tabs[0].1);
        assert_eq!(as_str(tab.iter().find(|(k, _)| as_str(k) == Some("name")).unwrap().1), Some("Fleet"));
        assert_eq!(as_str(tab.iter().find(|(k, _)| as_str(k) == Some("overview")).unwrap().1), Some("Friendly"));
        assert_eq!(as_str(tab.iter().find(|(k, _)| as_str(k) == Some("bracket")).unwrap().1), Some("Brackets"));

        // Only the palette-matching colour survives; the custom one is reported.
        let colors = pairs(pack.get("stateColorsNameList").unwrap());
        assert_eq!(colors.len(), 1);
        assert_eq!((as_str(colors[0].0), as_str(colors[0].1)), (Some("background_16"), Some("darkBlue")));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("1"), "warning counts the omitted colour: {warnings:?}");

        let blinks = pairs(pack.get("stateBlinks").unwrap());
        assert_eq!((as_str(blinks[0].0), blinks[0].1), (Some("flag_9"), &Node::Bool(true)));

        let settings = pairs(pack.get("userSettings").unwrap());
        assert_eq!((as_str(settings[0].0), settings[0].1), (Some("overviewBroadcastsToTop"), &Node::Bool(true)));
    }

    #[test]
    fn omits_sections_the_file_does_not_have() {
        let doc = Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("overviewColumns"), Value::List(vec![b("NAME")])),
        ]))]);
        let (pack, _) = read_pack(&doc);
        assert!(pack.get("presets").is_none());
        assert!(pack.get("shipLabels").is_none());
        assert_eq!(pack.sections.len(), 1);
    }

    /// A legacy account stores its tabs under `tabsettings`, not `tabsettings_new`
    /// (the two are structurally identical — see `overview_tabs::tabs_mut`, which
    /// migrates one to the other). Without this the `.or_else` fallback in the tab
    /// read could be deleted outright and the whole suite would stay green.
    #[test]
    fn reads_tabs_from_a_legacy_tabsettings_key() {
        let tab = Value::Dict(vec![
            (b("bracket"), b("Brackets")),
            (b("name"), Value::StrUcs2("Fleet".into())),
            (b("overview"), b("Friendly")),
        ]);
        let doc = Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("tabsettings"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
        ]))]);
        let (pack, _) = read_pack(&doc);
        let tabs = pairs(pack.get("tabSetup").expect("legacy tabs are read"));
        assert_eq!(tabs.len(), 1);
        let fields = pairs(tabs[0].1);
        assert_eq!(as_str(fields.iter().find(|(k, _)| as_str(k) == Some("name")).unwrap().1), Some("Fleet"));
    }

    #[test]
    fn a_read_pack_emits_and_reparses() {
        let (pack, _) = read_pack(&user_doc());
        let again = parse_pack(&emit_pack(&pack)).unwrap();
        assert_eq!(again.sections, pack.sections);
    }

    #[test]
    fn applies_lists_colours_blinks_and_bools() {
        let mut doc = user_doc();
        let pack = parse_pack(
            "backgroundStates:\n- 44\n- 9\nflagOrder:\n- 13\ncolumnOrder:\n- TYPE\n- NAME\n\
             stateColorsNameList:\n- - background_44\n  - red\nstateBlinks:\n- - flag_13\n  - true\n\
             userSettings:\n- - useSmallText\n  - true\n",
        ).unwrap();

        let report = apply_pack(&mut doc, &pack).unwrap();
        let (back, _) = read_pack(&doc);

        assert_eq!(ints(back.get("backgroundStates").unwrap()), vec![9, 44], "enabled lists are stored sorted");
        assert_eq!(ints(back.get("flagOrder").unwrap()), vec![13]);
        assert_eq!(strs(back.get("columnOrder").unwrap()), vec!["TYPE".to_string(), "NAME".to_string()],
                   "order lists keep the pack's order");

        let colors = pairs(back.get("stateColorsNameList").unwrap());
        assert_eq!(colors.len(), 1, "the pack's colours REPLACE the file's");
        assert_eq!((as_str(colors[0].0), as_str(colors[0].1)), (Some("background_44"), Some("red")));

        let blinks = pairs(back.get("stateBlinks").unwrap());
        assert_eq!((as_str(blinks[0].0), blinks[0].1), (Some("flag_13"), &Node::Bool(true)));

        let settings = pairs(back.get("userSettings").unwrap());
        assert!(settings.iter().any(|(k, v)| as_str(k) == Some("useSmallText") && *v == &Node::Bool(true)));
        assert!(report.applied.iter().any(|s| s == "backgroundStates"));
    }

    #[test]
    fn leaves_sections_the_pack_omits_untouched() {
        let mut doc = user_doc();
        let before = read_pack(&doc).0;
        let pack = parse_pack("backgroundStates:\n- 44\n").unwrap();
        apply_pack(&mut doc, &pack).unwrap();
        let after = read_pack(&doc).0;

        assert_eq!(after.get("overviewColumns"), before.get("overviewColumns"));
        assert_eq!(after.get("presets"), before.get("presets"));
        assert_eq!(after.get("stateColorsNameList"), before.get("stateColorsNameList"));
        assert_ne!(after.get("backgroundStates"), before.get("backgroundStates"));
    }

    #[test]
    fn skips_an_unknown_colour_name_and_an_unknown_setting() {
        let mut doc = user_doc();
        let pack = parse_pack(
            "stateColorsNameList:\n- - background_44\n  - chartreuse\n\
             userSettings:\n- - applyOnlyToShips\n  - true\n",
        ).unwrap();
        let report = apply_pack(&mut doc, &pack).unwrap();

        let (back, _) = read_pack(&doc);
        assert_eq!(pairs(back.get("stateColorsNameList").unwrap()).len(), 0, "unknown name writes nothing");
        assert!(report.warnings.iter().any(|w| w.contains("chartreuse")));
        assert!(report.warnings.iter().any(|w| w.contains("applyOnlyToShips")));
    }

    #[test]
    fn applying_a_pack_to_a_file_with_no_overview_container_errors() {
        let mut doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
        let pack = parse_pack("backgroundStates:\n- 44\n").unwrap();
        assert!(matches!(apply_pack(&mut doc, &pack), Err(PackError::NoOverview)));
    }

    /// The RAW value under a key in the document's (already-inlined) `overview`
    /// dict — no timestamp/Shared unwrapping, unlike `read_pack`'s deliberately
    /// permissive `unwrapped`. `read_pack` would happily accept a wrapper that
    /// should not be there (or a reset timestamp), so the structural write-path
    /// invariants below check the raw tree directly instead.
    fn raw_overview_entry<'a>(doc: &'a Value, key: &str) -> &'a Value {
        let Value::Dict(root) = doc else { panic!("doc is not a dict") };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).expect("overview container");
        let Value::Dict(entries) = ov else { panic!("overview is not a dict") };
        entries
            .iter()
            .find(|(k, _)| is_b(k, key.as_bytes()))
            .map(|(_, v)| v)
            .unwrap_or_else(|| panic!("missing key {key}"))
    }

    #[test]
    fn apply_pack_preserves_an_existing_wrappers_timestamp() {
        let mut doc = user_doc();
        let pack = parse_pack("backgroundStates:\n- 44\n").unwrap();
        apply_pack(&mut doc, &pack).unwrap();

        let Value::Tuple(items) = raw_overview_entry(&doc, "backgroundStates2") else {
            panic!("backgroundStates2 must stay a (timestamp, list) tuple");
        };
        assert_eq!(items[0], ts(), "an existing wrapper's own timestamp must survive, not be reset to zero");
    }

    /// Every container key in an EVE-written file is `(timestamp, payload)` —
    /// 4,187 of 4,187 across five untouched account files, columns included.
    /// This used to assert the opposite on the strength of a comment, with a
    /// fixture that seeded a bare list, so it passed while every real import
    /// stripped the wrapper off both column keys.
    #[test]
    fn apply_pack_wraps_every_list_section() {
        let mut doc = user_doc();
        let pack = parse_pack(
            "columnOrder:\n- TYPE\noverviewColumns:\n- TYPE\nbackgroundStates:\n- 44\n",
        ).unwrap();
        apply_pack(&mut doc, &pack).unwrap();

        for key in ["overviewColumnOrder", "overviewColumns", "backgroundStates2"] {
            let Value::Tuple(items) = raw_overview_entry(&doc, key) else {
                panic!("{key} must be a (timestamp, list) tuple, not a bare payload");
            };
            assert!(matches!(items[0], Value::Long(_)), "{key} keeps a timestamp first");
        }
    }

    /// A bare payload left by an older build is repaired, not perpetuated.
    #[test]
    fn apply_pack_rewraps_a_bare_payload() {
        let mut doc = user_doc();
        let pack = parse_pack("columnOrder:\n- TYPE\n").unwrap();
        apply_pack(&mut doc, &pack).unwrap();
        let Value::Tuple(_) = raw_overview_entry(&doc, "overviewColumnOrder") else {
            panic!("the fixture's bare overviewColumnOrder must come back wrapped");
        };
    }

    /// `apply_pack` rebuilds the file's ONE ship-label list by walking the pack's
    /// `shipLabelOrder` and pulling each body out of `shipLabels`, keyed by the
    /// label's own `type` field (the inverse of `read_pack`'s split, below). This
    /// is the only coverage of that reconstruction: it pins the pack's ORDER (a
    /// reversed walk must fail this test — the fixture keys `shipLabels` in a
    /// different order than `shipLabelOrder` so the two cannot be confused), a
    /// literal `null` label surviving (real published packs carry one — the
    /// bracket-only line), an order entry with no matching body ("ghost") and a
    /// body with no matching order entry ("orphan") both being skipped rather
    /// than corrupting the result, and each field landing with the type the file
    /// stores (`pre`/`post` as byte strings, `state` as an int, a `null` `type`
    /// as `Value::None`).
    ///
    /// `user_doc()` has no `shipLabels` key, so this is also the only test that
    /// exercises `put`'s "key absent" branch: the assertions on `wrapper[0]`/
    /// `wrapper.len()` check the RAW tree for a freshly-minted `(zero timestamp,
    /// list)` wrapper, because `read_pack`'s `unwrapped` accepts a bare value just
    /// as happily as a wrapped one and so cannot tell a lost wrapper apart.
    #[test]
    fn apply_pack_reconstructs_ship_labels_from_order_and_bodies() {
        fn fields(kvs: &[(&str, Node)]) -> Node {
            Node::Seq(kvs.iter().map(|(k, v)| Node::Seq(vec![Node::Str(k.to_string()), v.clone()])).collect())
        }
        fn keyed(entries: Vec<(Node, Node)>) -> Node {
            Node::Seq(entries.into_iter().map(|(k, v)| Node::Seq(vec![k, v])).collect())
        }
        fn field(entries: &Entries, name: &str) -> Value {
            entries
                .iter()
                .find(|(k, _)| is_b(k, name.as_bytes()))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("missing field '{name}'"))
        }

        let mut pack = Pack::default();
        pack.set(
            "shipLabelOrder",
            Node::Seq(vec![Node::Null, Node::Str("hull".into()), Node::Str("ghost".into())]),
        );
        pack.set(
            "shipLabels",
            keyed(vec![
                (
                    Node::Str("orphan".into()),
                    fields(&[("type", Node::Str("orphan".into())), ("pre", Node::Str("orphan-pre".into()))]),
                ),
                (
                    Node::Str("hull".into()),
                    fields(&[
                        ("type", Node::Str("hull".into())),
                        ("pre", Node::Str("hull-pre".into())),
                        ("post", Node::Str("hull-post".into())),
                        ("state", Node::Int(5)),
                    ]),
                ),
                (
                    Node::Null,
                    fields(&[
                        ("type", Node::Null),
                        ("pre", Node::Str("null-pre".into())),
                        ("post", Node::Str("null-post".into())),
                        ("state", Node::Int(2)),
                    ]),
                ),
            ]),
        );

        let mut doc = user_doc();
        apply_pack(&mut doc, &pack).unwrap();

        let Value::Tuple(wrapper) = raw_overview_entry(&doc, "shipLabels") else {
            panic!("shipLabels must be minted as a (timestamp, list) wrapper — the key is absent from user_doc()");
        };
        assert_eq!(wrapper.len(), 2, "a minted wrapper is exactly (timestamp, list): {wrapper:?}");
        assert_eq!(wrapper[0], Value::Long(vec![0u8; 8]), "an absent key mints a fresh ZERO timestamp");
        let Value::List(list) = &wrapper[1] else { panic!("wrapper's second element must be the label list") };
        assert_eq!(list.len(), 2, "ghost (no body) and orphan (no order entry) must both be skipped: {list:?}");

        let Value::Dict(first) = &list[0] else { panic!("label entry is a dict") };
        assert_eq!(field(first, "type"), Value::None, "the null label survives and decodes to None");
        assert_eq!(field(first, "pre"), b("null-pre"));
        assert_eq!(field(first, "post"), b("null-post"));
        assert_eq!(field(first, "state"), Value::Int(2));

        let Value::Dict(second) = &list[1] else { panic!("label entry is a dict") };
        assert_eq!(field(second, "type"), b("hull"), "the file list follows the pack's ORDER, not the bodies' own sequence");
        assert_eq!(field(second, "pre"), b("hull-pre"));
        assert_eq!(field(second, "post"), b("hull-post"));
        assert_eq!(field(second, "state"), Value::Int(5));
    }

    /// C2 regression guard: real labels carry `color` as a nested sequence of
    /// floats (the corpus has files with `color` as a `List` of floats). The
    /// generic field-value conversion inside a `filter_map` must not fall
    /// through to `_ => return None` for that shape — doing so drops the WHOLE
    /// field, not just leaves the colour unset.
    #[test]
    fn apply_pack_keeps_a_ship_labels_nested_color_field() {
        let mut pack = Pack::default();
        pack.set("shipLabelOrder", Node::Seq(vec![Node::Str("hull".into())]));
        pack.set("shipLabels", Node::Seq(vec![Node::Seq(vec![
            Node::Str("hull".into()),
            Node::Seq(vec![
                Node::Seq(vec![Node::Str("type".into()), Node::Str("hull".into())]),
                Node::Seq(vec![
                    Node::Str("color".into()),
                    Node::Seq(vec![Node::Float(1.0), Node::Float(1.0), Node::Float(1.0)]),
                ]),
            ]),
        ])]));

        let mut doc = user_doc();
        apply_pack(&mut doc, &pack).unwrap();

        let ov = crate::overview_tabs::overview_mut(&mut doc).unwrap();
        let (_, wrapper) = ov.iter().find(|(k, _)| is_b(k, b"shipLabels")).expect("shipLabels must be written");
        let Value::Tuple(items) = wrapper else { panic!("shipLabels must be a (timestamp, list) wrapper") };
        let Value::List(list) = &items[1] else { panic!("wrapper's second element must be the label list") };
        let Value::Dict(fields) = &list[0] else { panic!("label entry is a dict") };
        let (_, color) = fields.iter().find(|(k, _)| is_b(k, b"color")).expect("color field must survive, not be dropped");
        assert_eq!(color, &Value::List(vec![Value::Float(1.0), Value::Float(1.0), Value::Float(1.0)]));
    }

    /// `apply_pack` is documented to build every replacement value BEFORE the
    /// first mutation. A pack whose `shipLabelOrder` is not a sequence fails
    /// INSIDE the build phase — before `inline_all`/`overview_mut` ever run — so
    /// the document must come back unchanged. This proves atomicity for THAT
    /// build-phase failure only: it says nothing about the `NoOverview` path,
    /// which runs `inline_all` (a value-preserving Shared/Ref collapse) before
    /// returning its error, so a document can be structurally rewritten and
    /// still correctly be called "unchanged" there.
    #[test]
    fn apply_pack_leaves_the_document_unchanged_on_a_build_phase_failure() {
        let mut doc = user_doc();
        let before = doc.clone();
        let pack = parse_pack("shipLabelOrder: true\nshipLabels: []\n").unwrap();

        let err = apply_pack(&mut doc, &pack).unwrap_err();
        // Names the section rather than claiming the file holds no pack at all.
        assert!(matches!(&err, PackError::BadSection { name } if name == "shipLabelOrder"), "got {err:?}");
        assert!(err.to_string().contains("shipLabelOrder"), "the message names it too: {err}");
        assert_eq!(doc, before, "a build-phase failure must leave the document untouched");
    }

    fn user_doc_with_windows() -> Value {
        let Value::Dict(mut root) = user_doc() else { unreachable!() };
        let (_, ov) = root.iter_mut().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { unreachable!() };
        // two windows: window 0 holds tab 0, window 1 holds tab 7 (not in the pack)
        entries.push((b("tabsByWindowInstanceID"), Value::Tuple(vec![ts(), Value::List(vec![
            Value::List(vec![Value::Int(0)]),
            Value::List(vec![Value::Int(7)]),
        ])])));
        Value::Dict(root)
    }

    /// Same two windows as `user_doc_with_windows`, but each PER-WINDOW entry is
    /// itself a `(ts, list)` tuple rather than a bare list — a shape real files
    /// can carry alongside the outer wrapper, and one the fixture above never
    /// exercises.
    fn user_doc_with_wrapped_windows() -> Value {
        let Value::Dict(mut root) = user_doc() else { unreachable!() };
        let (_, ov) = root.iter_mut().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { unreachable!() };
        entries.push((b("tabsByWindowInstanceID"), Value::Tuple(vec![ts(), Value::List(vec![
            Value::Tuple(vec![ts(), Value::List(vec![Value::Int(0)])]),
            Value::Tuple(vec![ts(), Value::List(vec![Value::Int(7)])]),
        ])])));
        Value::Dict(root)
    }

    /// The real shape a corpus review found on real multi-window accounts: three
    /// windows, tabs split 6+1+1 (window 0 holds everything else, windows 1 and
    /// 2 each hold one tab). `apply_tabs`'s old bug gave window 0 every pack
    /// index up front, so tabs 5 and 6 ended up living in two windows at once —
    /// this fixture is what `reapplying_the_full_tab_set_...` below re-imports
    /// a matching pack onto, to pin that it no longer happens.
    fn user_doc_with_three_windows() -> Value {
        let Value::Dict(mut root) = user_doc() else { unreachable!() };
        let (_, ov) = root.iter_mut().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { unreachable!() };
        entries.push((b("tabsByWindowInstanceID"), Value::Tuple(vec![ts(), Value::List(vec![
            Value::List([0, 1, 2, 3, 4, 7].into_iter().map(Value::Int).collect()),
            Value::List(vec![Value::Int(5)]),
            Value::List(vec![Value::Int(6)]),
        ])])));
        Value::Dict(root)
    }

    /// Unwrap a bare list OR a `(ts, list)` tuple — mirrors the unwrap `apply_tabs`
    /// itself performs on both the outer mapping value and each per-window entry.
    fn list_or_wrapped(v: &Value) -> &Vec<Value> {
        match v {
            Value::List(l) => l,
            Value::Tuple(items) => items.iter().find_map(|e| match e { Value::List(l) => Some(l), _ => None })
                .expect("tuple has no inner list"),
            _ => panic!("expected a list or (ts, list) tuple, got {v:?}"),
        }
    }

    fn as_ints(items: &[Value]) -> Vec<i64> {
        items.iter().filter_map(crate::overview_tabs::as_int).collect()
    }

    // A raw string (matching FIXTURE's convention above), not a `\`-continued
    // literal: a `\<newline>` in a Rust string literal eats not just the
    // newline but ALL leading whitespace of the following physical line, which
    // would silently swallow the 2-space indent block YAML needs here.
    const TWO_TAB_PACK: &str = r#"presets:
- - Enemies
  - - - alwaysShownStates
      - []
    - - filteredStates
      - []
    - - groups
      - - 27
tabSetup:
- - 0
  - - - bracket
      - Enemies
    - - name
      - Tab A
    - - overview
      - Enemies
- - 1
  - - - bracket
      - Enemies
    - - name
      - Tab B
    - - overview
      - Enemies
"#;

    #[test]
    fn replaces_presets_and_tabs() {
        let mut doc = user_doc();
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();
        let (back, _) = read_pack(&doc);

        let presets = pairs(back.get("presets").unwrap());
        assert_eq!(presets.len(), 1, "the file's own preset is gone");
        assert_eq!(as_str(presets[0].0), Some("Enemies"));

        let tabs = pairs(back.get("tabSetup").unwrap());
        assert_eq!(tabs.len(), 2);
        let names: Vec<Option<&str>> = tabs.iter()
            .map(|(_, body)| as_str(pairs(body).iter().find(|(k, _)| as_str(k) == Some("name")).unwrap().1))
            .collect();
        assert_eq!(names, vec![Some("Tab A"), Some("Tab B")]);
    }

    #[test]
    fn a_new_tab_keeps_the_color_key_reset_needs() {
        let mut doc = user_doc();
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();
        // Tab 1 did not exist before; it must be a clone of tab 0, so it carries
        // `color` — EVE's reset-overview iterates tabs reading it.
        let mut probe = doc.clone();
        let ov = crate::overview_tabs::overview_mut(&mut probe).unwrap();
        let tabs = crate::overview_tabs::tabs_mut(ov);
        for idx in [0i64, 1] {
            let (_, tab) = tabs.iter_mut().find(|(k, _)| crate::overview_tabs::as_int(k) == Some(idx)).unwrap();
            let fields = crate::overview_tabs::dict_inner_mut(tab).unwrap();
            assert!(fields.iter().any(|(k, _)| is_b(k, b"color")), "tab {idx} lost its color key");
        }
    }

    #[test]
    fn a_new_tab_drops_per_tab_column_overrides() {
        // tab 0 in user_doc() carries no tabColumns/tabColumnOrder, so give it
        // some before applying the pack — the clone must not carry them onto the
        // new tab (or back onto tab 0 itself), since pack columns are account-global.
        let mut doc = user_doc();
        {
            let ov = crate::overview_tabs::overview_mut(&mut doc).unwrap();
            let tabs = crate::overview_tabs::tabs_mut(ov);
            let (_, tab0) = tabs.iter_mut().find(|(k, _)| crate::overview_tabs::as_int(k) == Some(0)).unwrap();
            let fields = crate::overview_tabs::dict_inner_mut(tab0).unwrap();
            fields.push((b("tabColumns"), Value::List(vec![b("NAME")])));
            fields.push((b("tabColumnOrder"), Value::List(vec![b("NAME")])));
        }
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();

        let mut probe = doc.clone();
        let ov = crate::overview_tabs::overview_mut(&mut probe).unwrap();
        let tabs = crate::overview_tabs::tabs_mut(ov);
        for idx in [0i64, 1] {
            let (_, tab) = tabs.iter_mut().find(|(k, _)| crate::overview_tabs::as_int(k) == Some(idx)).unwrap();
            let fields = crate::overview_tabs::dict_inner_mut(tab).unwrap();
            assert!(!fields.iter().any(|(k, _)| is_b(k, b"tabColumns")), "tab {idx} kept a per-tab tabColumns override");
            assert!(!fields.iter().any(|(k, _)| is_b(k, b"tabColumnOrder")), "tab {idx} kept a per-tab tabColumnOrder override");
        }
    }

    #[test]
    fn rebuilds_the_window_mapping_without_dangling_indices() {
        let mut doc = user_doc_with_windows();
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();

        let mut probe = doc.clone();
        let ov = crate::overview_tabs::overview_mut(&mut probe).unwrap();
        let (_, groups_val) = ov.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let text = format!("{groups_val:?}");
        assert!(text.contains("Int(0)") && text.contains("Int(1)"), "both pack tabs are mapped: {text}");
        assert!(!text.contains("Int(7)"), "the dangling index is gone: {text}");

        // Direct assertions on the actual lists, not just substring matching on
        // a debug-formatted blob: window 1 must become an EMPTY list, and the
        // window itself must still be present (not removed).
        let groups = list_or_wrapped(groups_val);
        assert_eq!(groups.len(), 2, "window 1 must stay present, not be removed");
        assert_eq!(as_ints(list_or_wrapped(&groups[0])), vec![0, 1], "window 0 gets both pack tabs");
        assert!(list_or_wrapped(&groups[1]).is_empty(), "window 1's only index (7) was dangling: must leave an EMPTY list");
    }

    #[test]
    fn rebuilds_wrapped_per_window_entries_without_dangling_indices() {
        // Same scenario as above, but each per-window entry is itself a
        // `(ts, list)` tuple — the inner-tuple branch the plain-list fixture
        // never exercises.
        let mut doc = user_doc_with_wrapped_windows();
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();

        let mut probe = doc.clone();
        let ov = crate::overview_tabs::overview_mut(&mut probe).unwrap();
        let (_, groups_val) = ov.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let groups = list_or_wrapped(groups_val);
        assert_eq!(groups.len(), 2, "window 1 must stay present, not be removed");

        assert!(matches!(&groups[0], Value::Tuple(_)), "window 0's own (ts, list) wrapper must survive");
        assert_eq!(as_ints(list_or_wrapped(&groups[0])), vec![0, 1], "window 0 gets both pack tabs");

        let Value::Tuple(w1_items) = &groups[1] else { panic!("window 1's own (ts, list) wrapper must survive") };
        assert_eq!(w1_items[0], ts(), "the per-window tuple's own timestamp must survive untouched");
        assert!(list_or_wrapped(&groups[1]).is_empty(), "window 1's only index (7) was dangling: must leave an EMPTY list");
    }

    /// The corpus-measured C1 regression guard: re-importing a pack that names
    /// exactly the tabs the account already had (an export-then-re-import,
    /// which changes nothing) must reproduce the SAME mapping, not duplicate an
    /// index into two windows. This is the permanent guard on the bug where
    /// window 0 was given every pack index before the secondary windows were
    /// filtered, so whatever a secondary window also kept ended up in both.
    #[test]
    fn reapplying_the_full_tab_set_does_not_duplicate_an_index_across_windows() {
        let mut doc = user_doc_with_three_windows();
        let mut pack = Pack::default();
        let tabs: Vec<Node> = [0, 1, 2, 3, 4, 5, 6, 7].iter().map(|&i| Node::Seq(vec![
            Node::Int(i),
            Node::Seq(vec![Node::Seq(vec![Node::Str("name".into()), Node::Str(format!("Tab {i}"))])]),
        ])).collect();
        pack.set("tabSetup", Node::Seq(tabs));

        apply_pack(&mut doc, &pack).unwrap();

        let mut probe = doc.clone();
        let ov = crate::overview_tabs::overview_mut(&mut probe).unwrap();
        let (_, groups_val) = ov.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let groups = list_or_wrapped(groups_val);
        assert_eq!(groups.len(), 3, "all three windows stay present");

        let per_window: Vec<Vec<i64>> = groups.iter().map(|g| as_ints(list_or_wrapped(g))).collect();
        assert_eq!(
            per_window,
            vec![vec![0, 1, 2, 3, 4, 7], vec![5], vec![6]],
            "re-importing the account's own full tab set must leave the mapping unchanged: {per_window:?}",
        );

        let mut seen: Vec<i64> = Vec::new();
        for idx in per_window.into_iter().flatten() {
            assert!(!seen.contains(&idx), "index {idx} appears in more than one window");
            seen.push(idx);
        }
    }

    #[test]
    fn warns_when_a_pack_leaves_overview_windows_with_no_tabs() {
        // The pack only defines the six tabs window 0 held; windows 1 and 2
        // (whose only tab was 5 and 6 respectively) end up empty. There is no
        // way to avoid this — a pack carries no window model — so it must be
        // reported rather than left for the user to discover in-game.
        let mut doc = user_doc_with_three_windows();
        let mut pack = Pack::default();
        let tabs: Vec<Node> = [0, 1, 2, 3, 4, 7].iter().map(|&i| Node::Seq(vec![
            Node::Int(i),
            Node::Seq(vec![Node::Seq(vec![Node::Str("name".into()), Node::Str(format!("Tab {i}"))])]),
        ])).collect();
        pack.set("tabSetup", Node::Seq(tabs));

        let report = apply_pack(&mut doc, &pack).unwrap();
        assert!(
            report.warnings.iter().any(|w| w.contains('2') && w.contains("no tabs")),
            "warns how many windows ended up empty: {:?}", report.warnings,
        );
    }

    #[test]
    fn never_fabricates_a_window_mapping() {
        let mut doc = user_doc(); // no tabsByWindowInstanceID
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();
        let mut probe = doc.clone();
        let ov = crate::overview_tabs::overview_mut(&mut probe).unwrap();
        assert!(!ov.iter().any(|(k, _)| is_b(k, b"tabsByWindowInstanceID")),
                "a windowless account must stay windowless");
    }

    #[test]
    fn replacing_presets_drops_the_stale_not_saved_working_copy() {
        // A stale overviewProfilePresets_notSaved entry keyed by a name the new
        // pack doesn't define would resurrect a phantom preset in-game.
        let mut doc = user_doc();
        {
            let ov = crate::overview_tabs::overview_mut(&mut doc).unwrap();
            ov.push((b("overviewProfilePresets_notSaved"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("Friendly"), Value::Dict(vec![])),
            ])])));
        }
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();

        let Value::Dict(root) = &doc else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        assert!(!entries.iter().any(|(k, _)| is_b(k, b"overviewProfilePresets_notSaved")),
                "a stale notSaved working copy must not survive a preset replacement");
    }

    #[test]
    fn zero_tab_account_fallback_carries_bracket_and_color() {
        // No tab at all to clone as a template: apply_tabs falls back to
        // overview_tabs::create_tab's own no-sibling default. The pack's tab body
        // supplies only a name, so `bracket` can ONLY come from the fallback
        // itself — a fallback that carries `color` but not `bracket` (the bug)
        // would leave tab 0 with no `bracket` key at all.
        let mut doc = Value::Dict(vec![(b("overview"), Value::Dict(vec![]))]);
        let mut pack = Pack::default();
        pack.set("tabSetup", Node::Seq(vec![Node::Seq(vec![
            Node::Int(0),
            Node::Seq(vec![Node::Seq(vec![Node::Str("name".into()), Node::Str("Solo".into())])]),
        ])]));
        apply_pack(&mut doc, &pack).unwrap();

        // Read the RAW tree directly (not through read_pack/pairs, which would
        // just re-project whatever keys happen to exist).
        let ov = crate::overview_tabs::overview_mut(&mut doc).unwrap();
        let tabs = crate::overview_tabs::tabs_mut(ov);
        let (_, tab) = tabs.iter_mut().find(|(k, _)| crate::overview_tabs::as_int(k) == Some(0)).unwrap();
        let fields = crate::overview_tabs::dict_inner_mut(tab).unwrap();
        assert!(fields.iter().any(|(k, _)| is_b(k, b"bracket")), "fallback tab must carry bracket: {fields:?}");
        assert!(fields.iter().any(|(k, _)| is_b(k, b"color")), "fallback tab must carry color: {fields:?}");
    }

    #[test]
    fn skips_an_empty_presets_section_and_warns() {
        let mut doc = user_doc();
        let before_presets = read_pack(&doc).0.get("presets").cloned();
        let pack = parse_pack("presets: []\nbackgroundStates:\n- 44\n").unwrap();
        let report = apply_pack(&mut doc, &pack).unwrap();

        let (back, _) = read_pack(&doc);
        assert_eq!(back.get("presets"), before_presets.as_ref(), "the account's own preset(s) survive untouched");
        assert!(report.warnings.iter().any(|w| w.contains("presets")), "warns about the skipped section: {:?}", report.warnings);
        assert!(!report.applied.iter().any(|s| s == "presets"), "presets must not be reported as applied");
        assert!(report.applied.iter().any(|s| s == "backgroundStates"), "a good section in the same pack still applies");
    }

    #[test]
    fn skips_an_empty_tab_setup_section_and_warns() {
        let mut doc = user_doc();
        let before_tabs = read_pack(&doc).0.get("tabSetup").cloned();
        let pack = parse_pack("tabSetup: []\nbackgroundStates:\n- 44\n").unwrap();
        let report = apply_pack(&mut doc, &pack).unwrap();

        let (back, _) = read_pack(&doc);
        assert_eq!(back.get("tabSetup"), before_tabs.as_ref(), "the account's own tab(s) survive untouched");
        assert!(report.warnings.iter().any(|w| w.contains("tabSetup")), "warns about the skipped section: {:?}", report.warnings);
        assert!(!report.applied.iter().any(|s| s == "tabSetup"), "tabSetup must not be reported as applied");
        assert!(report.applied.iter().any(|s| s == "backgroundStates"), "a good section in the same pack still applies");
    }
}
