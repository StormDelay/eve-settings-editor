# Overview import/export packs (slice 4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import and export EVE's own overview pack YAML on the open character's account, so a downloaded community pack can be applied out of game and an account's overview can be shared as a pack EVE itself loads.

**Architecture:** One new crate module, `settings-model/src/overview_pack.rs`, owns all pack-format knowledge: a tiny YAML node type, a parser (`yaml-rust2`), a hand-written emitter, and the two conversions between a pack and the `core_user` → `overview` container. The app layer adds three thin commands routed through the existing `edit_user_tabs` idiom (lock → inline-first → mutate → `reshare` → re-project → mark dirty), and the Overview view grows two buttons. Import marks the slot dirty; the user still presses Save, so the normal backup chain applies.

**Tech Stack:** Rust (workspace crates `blue-marshal`, `settings-model`, `app/src-tauri`), `yaml-rust2` for parsing, Tauri 2 commands, Svelte 5 runes frontend, `@tauri-apps/plugin-dialog` for the file pickers.

**Spec:** `docs/superpowers/specs/2026-07-25-overview-import-export-packs-design.md`

## Global Constraints

- **`blue-marshal` stays dependency-free.** The YAML dependency lands in `settings-model` only.
- **Read paths must resolve `Shared`/`Ref`.** Real `core_user` files intern repeated values; a read that matches bare `Value::Bytes`/`Value::List` passes every hand-built unit test and reads *nothing* from a real file. Use `collect_shared` + `effective` at every hop (the slice-3 bug).
- **Write paths are inline-first.** Call `inline_all(v)` before structural edits; the app layer reshares afterwards. Never hand-maintain `Shared`/`Ref` while editing.
- **Never fabricate `tabsByWindowInstanceID`.** An absent mapping means EVE distributes tabs itself; an empty or partial mapping hides the whole overview.
- **Every real tab carries `bracket` and `color`.** EVE's "reset all overview settings" iterates tabs reading them, so a tab dict missing them makes the reset throw. Build new tabs by cloning an existing tab dict, as `create_tab` does.
- **Minted `(timestamp, value)` wrappers use a zero `Long`** (`Value::Long(vec![0u8; 8])`), matching `overview_states.rs`. EVE accepts a freshly minted zero-timestamp container.
- **No personal data in the repo.** No character/account ids, no real character names, no live-directory paths in code, tests, comments or docs. Test fixtures are synthetic.
- **Commits are sentence-case with no attribution trailers** (e.g. `Parse an overview pack into a section tree`).
- **The pack node type is re-exported as `PackNode`.** `settings-model`'s root already exports `projection::Node`, so `pub use overview_pack::{Node as PackNode, …}`. Inside `overview_pack.rs` the type is still plain `Node`; only the external name is aliased. (Found in Task 1.)
- Rust tests: `cargo test -p settings-model`. Frontend: `cd app && npm test`, `npm run check`.

**Before Task 1:** branch off master — `git checkout -b overview-import-export-packs` — as every previous slice did. The spec is already committed on master.

---

### Task 1: Pack node model and parser

**Files:**
- Create: `crates/settings-model/src/overview_pack.rs`
- Modify: `crates/settings-model/Cargo.toml`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub enum Node { Null, Bool(bool), Int(i64), Float(f64), Str(String), Seq(Vec<Node>) }`
  - `pub struct Pack { pub sections: Vec<(String, Node)> }` with `pub fn get(&self, name: &str) -> Option<&Node>` and `pub fn set(&mut self, name: &str, node: Node)`
  - `pub fn parse_pack(text: &str) -> Result<Pack, PackError>`
  - `pub enum PackError { Yaml { message: String }, NotAMapping, NotAPack }` — **struct variants only**, `#[serde(tag = "code", rename_all = "snake_case")]` plus a `Display` impl, exactly like `OverviewTabError`. (An internally-tagged serde enum cannot serialize a newtype variant holding a `String`, so `Yaml(String)` would fail at runtime.)
  - `pub const SECTIONS: [&str; 13]` — the recognised section names
  - Helpers used by later tasks: `pub fn ints(n: &Node) -> Vec<i64>`, `pub fn strs(n: &Node) -> Vec<String>`, `pub fn pairs(n: &Node) -> Vec<(&Node, &Node)>`, `pub fn as_str(n: &Node) -> Option<&str>`

- [ ] **Step 1: Add the dependency**

In `crates/settings-model/Cargo.toml`, under `[dependencies]`:

```toml
yaml-rust2 = "0.10"
```

If cargo reports no matching version, take the latest 0.x it offers — the only API used here is `YamlLoader::load_from_str` and the `Yaml` enum, stable since 0.6. Run `cargo fetch` and confirm it resolves.

- [ ] **Step 2: Write the failing test**

Create `crates/settings-model/src/overview_pack.rs` containing ONLY this test module for now (the rest of the file arrives in Step 4):

```rust
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
}
```

Register the module in `crates/settings-model/src/lib.rs`, beside the other overview modules:

```rust
mod overview_pack;
```

and add the re-export line (extend it in later tasks):

```rust
pub use overview_pack::{parse_pack, Node, Pack, PackError, SECTIONS};
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p settings-model overview_pack`
Expected: FAIL to compile — `cannot find function parse_pack`, `cannot find type Node`.

- [ ] **Step 4: Write the implementation**

Put this ABOVE the `#[cfg(test)] mod tests` block in `crates/settings-model/src/overview_pack.rs`:

```rust
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
pub const SECTIONS: [&str; 13] = [
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
    /// The document has no `overview` container to write into.
    NoOverview,
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::Yaml { message } => write!(f, "This file is not valid YAML: {message}"),
            PackError::NotAMapping => write!(f, "This file is not an overview pack."),
            PackError::NotAPack => write!(f, "This YAML file contains no overview pack sections."),
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

/// Parse a pack. Tolerates a leading UTF-8 BOM (real packs carry one).
pub fn parse_pack(text: &str) -> Result<Pack, PackError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p settings-model overview_pack`
Expected: PASS (5 tests). Also run `cargo test -p settings-model` for the whole crate — expected: all green.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/Cargo.toml crates/settings-model/src/overview_pack.rs crates/settings-model/src/lib.rs Cargo.lock
git commit -m "Parse an overview pack into a section tree"
```

---

### Task 2: Pack emitter

**Files:**
- Modify: `crates/settings-model/src/overview_pack.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: `Node`, `Pack`, `parse_pack`, `SECTIONS` (Task 1).
- Produces: `pub fn emit_pack(pack: &Pack) -> String` — valid YAML, no BOM, sections in `SECTIONS` order.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/settings-model/src/overview_pack.rs`:

```rust
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
        let text = emit_pack(&pack);
        let again = parse_pack(&text).unwrap();
        let name = pairs(again.get("presets").unwrap())[0].0;
        assert_eq!(as_str(name), Some("It's <b>bold</b>"));
        assert!(text.contains("[]"), "an empty sequence emits as []: {text}");
    }

    #[test]
    fn round_trips_a_multiline_scalar() {
        let mut pack = Pack::default();
        pack.set("columnOrder", Node::Seq(vec![Node::Str("two\nlines".into())]));
        let again = parse_pack(&emit_pack(&pack)).unwrap();
        assert_eq!(strs(again.get("columnOrder").unwrap()), vec!["two\nlines".to_string()]);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p settings-model overview_pack`
Expected: FAIL to compile — `cannot find function emit_pack`.

- [ ] **Step 3: Write the implementation**

Add to `crates/settings-model/src/overview_pack.rs`:

```rust
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
```

Extend the re-export in `crates/settings-model/src/lib.rs`:

```rust
pub use overview_pack::{emit_pack, parse_pack, Node, Pack, PackError, SECTIONS};
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p settings-model overview_pack`
Expected: PASS (9 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_pack.rs crates/settings-model/src/lib.rs
git commit -m "Emit an overview pack as YAML"
```

---

### Task 3: Colour palette and surface keys

Packs name colours (`darkBlue`); the file stores RGBA. The mapping is harvested from the corpus: a file that imported a pack carries the pack verbatim under `overview` → `restoreData` → `data` (in the PACK's vocabulary, including `stateColorsNameList`) and the resulting RGBA under `overview` → `stateColors`. Joining the two yields the palette with no in-game capture.

**Files:**
- Create: `crates/settings-model/src/bin/pack_palette.rs` (research bin, same throwaway convention as `overview_dump.rs`)
- Modify: `crates/settings-model/src/overview_pack.rs`
- Modify: `docs/format-notes.md`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub fn color_rgba(name: &str) -> Option<[f64; 4]>`
  - `pub fn color_name(rgba: [f64; 4]) -> Option<&'static str>`
  - `pub(crate) fn split_surface_key(s: &str) -> Option<(&str, i64)>` — `"background_16"` → `("background", 16)`
  - `pub(crate) fn join_surface_key(surface: &str, id: i64) -> String`

- [ ] **Step 1: Write the research bin**

Create `crates/settings-model/src/bin/pack_palette.rs`:

```rust
// Throwaway research tool: harvest EVE's overview-pack colour palette
// (name -> RGBA) from the corpus. A file whose account imported a pack keeps
// the pack verbatim under overview->restoreData->data (pack vocabulary,
// including stateColorsNameList) and the RGBA EVE derived from it under
// overview->stateColors. Joining the two gives the palette.
//
// usage: cargo run -p settings-model --bin pack_palette -- <corpus-dir>
use std::collections::BTreeMap;
use std::path::Path;
use blue_marshal::Value;

fn dict<'a>(v: &'a Value, key: &[u8]) -> Option<&'a Value> {
    let Value::Dict(d) = v else { return None };
    d.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b.as_slice() == key)).map(|(_, v)| v)
}

fn inner<'a>(v: &'a Value) -> &'a Value {
    match v {
        Value::Tuple(items) => items.iter().find(|e| matches!(e, Value::Dict(_) | Value::List(_))).unwrap_or(v),
        other => other,
    }
}

fn walk(path: &Path, out: &mut BTreeMap<String, [String; 4]>) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let Ok(v) = blue_marshal::decode(&bytes) else { return };
    let flat = blue_marshal::inline(&v);
    let Some(ov) = dict(&flat, b"overview") else { return };
    let ov = inner(ov);

    // names: restoreData -> data -> stateColorsNameList
    let mut names: BTreeMap<i64, String> = BTreeMap::new();
    if let Some(rd) = dict(ov, b"restoreData") {
        if let Some(data) = dict(inner(rd), b"data") {
            if let Some(Value::List(list)) = dict(inner(data), b"stateColorsNameList") {
                for e in list {
                    let Value::Tuple(kv) = e else { continue };
                    let [k, val] = kv.as_slice() else { continue };
                    let (Value::Bytes(k), Value::Bytes(val)) = (k, val) else { continue };
                    let k = String::from_utf8_lossy(k).into_owned();
                    let Some(id) = k.strip_prefix("background_").and_then(|n| n.parse::<i64>().ok()) else { continue };
                    names.insert(id, String::from_utf8_lossy(val).into_owned());
                }
            }
        }
    }
    if names.is_empty() { return; }

    // rgba: stateColors -> {(b"background", id): (f,f,f,f)}
    let Some(sc) = dict(ov, b"stateColors") else { return };
    let Value::Dict(entries) = inner(sc) else { return };
    for (k, val) in entries {
        let Value::Tuple(kp) = k else { continue };
        let [Value::Bytes(surface), Value::Int(id)] = kp.as_slice() else { continue };
        if surface.as_slice() != b"background" { continue }
        let Value::Tuple(rgba) = val else { continue };
        let parts: Vec<String> = rgba.iter().map(|c| match c {
            Value::Float(f) => format!("{f:?}"),
            Value::Int(i) => format!("{:?}", *i as f64),
            _ => "?".into(),
        }).collect();
        let [r, g, b, a] = parts.as_slice() else { continue };
        if let Some(name) = names.get(id) {
            out.insert(name.clone(), [r.clone(), g.clone(), b.clone(), a.clone()]);
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("usage: pack_palette <corpus-dir>");
    let mut out = BTreeMap::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(p) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&p) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() { stack.push(path); }
            else if path.file_name().map_or(false, |n| n.to_string_lossy().starts_with("core_user_")) {
                walk(&path, &mut out);
            }
        }
    }
    for (name, [r, g, b, a]) in out {
        println!("(\"{name}\", [{r}, {g}, {b}, {a}]),");
    }
}
```

- [ ] **Step 2: Run it over the corpus**

Run: `cargo run -q -p settings-model --bin pack_palette -- testdata/corpus`
Expected: a handful of lines like `("darkBlue", [0.0, 0.15, 0.6, 1.0]),`. The three pairs confirmed while designing MUST appear: `darkBlue` `[0.0, 0.15, 0.6, 1.0]`, `blue` `[0.2, 0.5, 1.0, 1.0]`, `red` `[0.75, 0.0, 0.0, 1.0]`. If a name maps to two different RGBAs across files, keep the value that appears in the most files and note the conflict in the table comment.

- [ ] **Step 3: Write the failing test**

Add to the `tests` module in `crates/settings-model/src/overview_pack.rs`:

```rust
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
        assert_eq!(join_surface_key("flag", 9), "flag_9".to_string());
    }
```

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test -p settings-model overview_pack`
Expected: FAIL to compile — `cannot find function color_rgba`.

- [ ] **Step 5: Write the implementation**

Add to `crates/settings-model/src/overview_pack.rs`. The `PALETTE` body below holds only the three rows confirmed during design — **replace it with the full harvested output from Step 2**, kept sorted by name, with the three rows below appearing verbatim among them, and set the array length in the type to the harvested row count:

```rust
/// EVE's overview colour palette: the names a pack uses for a state's row
/// colour, and the RGBA the client writes for each.
///
/// HARVESTED FROM THE CORPUS, not from a client data file: an account that
/// imported a pack keeps the pack's `stateColorsNameList` under
/// `overview`→`restoreData`→`data` and the RGBA EVE derived from it under
/// `overview`→`stateColors`; joining them across the corpus yields this table
/// (`src/bin/pack_palette.rs`). A name absent here is skipped on import and a
/// colour absent here is omitted on export — never approximated, since a
/// near-miss would silently change the user's colours.
pub(crate) const PALETTE: [(&str, [f64; 4]); 3] = [
    ("blue", [0.2, 0.5, 1.0, 1.0]),
    ("darkBlue", [0.0, 0.15, 0.6, 1.0]),
    ("red", [0.75, 0.0, 0.0, 1.0]),
];

pub fn color_rgba(name: &str) -> Option<[f64; 4]> {
    PALETTE.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Exact match only. Two floats that differ in the last bit are not the same
/// colour name, and guessing the nearest one would rewrite a user's colours.
pub fn color_name(rgba: [f64; 4]) -> Option<&'static str> {
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
```

Extend the re-export in `crates/settings-model/src/lib.rs`:

```rust
pub use overview_pack::{color_name, color_rgba, emit_pack, parse_pack, Node, Pack, PackError, SECTIONS};
```

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p settings-model overview_pack`
Expected: PASS (12 tests).

- [ ] **Step 7: Record the finding in the format notes**

Append to `docs/format-notes.md`, in the overview section:

```markdown
- Overview **packs** (the YAML EVE's Overview Settings → Misc Import/Export
  writes) encode dicts as sequences of two-element `[key, value]` sequences and
  use a vocabulary of their own: `backgroundStates`/`backgroundOrder`/
  `flagStates`/`flagOrder` (the file's `…2` keys without the suffix),
  `columnOrder` (`overviewColumnOrder`), `presets`
  (`overviewProfilePresets` — bracket presets are ordinary presets, referenced
  by a tab's `bracket` field), `tabSetup` (`tabsettings_new`),
  `stateColorsNameList` (`stateColors`, by palette NAME not RGBA),
  `stateBlinks`, `shipLabels`/`shipLabelOrder` and `userSettings`.
- `overview` → `restoreData` → `data` holds the LAST IMPORTED PACK verbatim in
  that pack vocabulary, and `presetHistoryKeys` is the imported-pack MRU (keyed
  by content hash, `overviewName` = the pack's name). Joining `restoreData`'s
  `stateColorsNameList` against the live `stateColors` is how the colour palette
  (`darkBlue` = `(0.0, 0.15, 0.6, 1.0)`, …) was derived — see
  `overview_pack.rs` `PALETTE`. Both keys stay read-only for us.
```

- [ ] **Step 8: Commit**

```bash
git add crates/settings-model/src/bin/pack_palette.rs crates/settings-model/src/overview_pack.rs crates/settings-model/src/lib.rs docs/format-notes.md
git commit -m "Harvest the overview pack colour palette from the corpus"
```

---

### Task 4: Read a pack out of a settings file (export side)

**Files:**
- Modify: `crates/settings-model/src/overview_pack.rs`
- Modify: `crates/settings-model/src/lib.rs`
- Create: `crates/settings-model/tests/overview_pack_realshape.rs`

**Interfaces:**
- Consumes: `Node`, `Pack`, `color_name`, `join_surface_key` (Tasks 1–3); `treewalk::{collect_shared, effective, Entries, SharedTable}`.
- Produces:
  - `pub fn read_pack(v: &Value) -> (Pack, Vec<String>)` — the pack plus warnings (e.g. `"2 custom colours had no pack name and were omitted"`).

Read paths MUST resolve `Shared`/`Ref` (Global Constraints). This task's realshape test is the guard.

- [ ] **Step 1: Write the failing unit test**

Add to the `tests` module in `crates/settings-model/src/overview_pack.rs`:

```rust
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
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

    #[test]
    fn a_read_pack_emits_and_reparses() {
        let (pack, _) = read_pack(&user_doc());
        let again = parse_pack(&emit_pack(&pack)).unwrap();
        assert_eq!(again.sections, pack.sections);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p settings-model overview_pack`
Expected: FAIL to compile — `cannot find function read_pack`.

- [ ] **Step 3: Write the implementation**

Add to `crates/settings-model/src/overview_pack.rs` (imports at the top of the file):

```rust
use blue_marshal::Value;

use crate::overview_states::OVERVIEW_BOOLS;
use crate::treewalk::{collect_shared, effective, Entries, SharedTable};
```

and the read path:

```rust
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

/// The `userSettings` names this build understands, paired with the file key.
/// Packs also carry names with no key on current files (`applyOnlyToShips`, an
/// older single toggle that became `applyToStructures`/`applyToOtherObjects`);
/// those are IGNORED rather than minted, and `set_overview_bool`'s allow-list is
/// the backstop.
const USER_SETTINGS: [(&str, &str); 6] = [
    ("applyToStructures", "applyToStructures"),
    ("applyToOtherObjects", "applyToOtherObjects"),
    ("useSmallColorTags", "useSmallColorTags"),
    ("useSmallText", "useSmallText"),
    ("overviewBroadcastsToTop", "overviewBroadcastsToTop"),
    ("hideCorpTicker", "hideCorpTicker"),
];

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
    let settings: Vec<Node> = USER_SETTINGS
        .iter()
        .filter_map(|(pack_name, file_key)| {
            let raw = find(ov, file_key.as_bytes(), &sh)?;
            let Value::Bool(on) = unwrapped(raw, &sh) else { return None };
            Some(Node::Seq(vec![Node::Str(pack_name.to_string()), Node::Bool(*on)]))
        })
        .collect();
    if !settings.is_empty() {
        pack.set("userSettings", Node::Seq(settings));
    }

    (pack, warnings)
}
```

Note `OVERVIEW_BOOLS` is imported for the Task 5 write path; if the compiler warns it is unused in this task, add the import in Task 5 instead.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p settings-model overview_pack`
Expected: PASS (15 tests).

- [ ] **Step 5: Write the realshape guard**

Create `crates/settings-model/tests/overview_pack_realshape.rs`:

```rust
//! Real-idiom guard for the pack READ path. On real `core_user` files every
//! repeated value is interned as `Shared`/`Ref` — the preset field names, the
//! `background` surface string, the state lists. A `read_pack` that matched
//! bare `Value::Bytes`/`Value::List` would pass every hand-built unit test in
//! `overview_pack.rs` and export an EMPTY pack from a real file (the exact bug
//! slice 3 shipped and had to fix). Fully synthetic: no bytes here came from a
//! real file.

use blue_marshal::{decode, encode, Value};
use settings_model::{emit_pack, parse_pack, read_pack};

// NOTE: the pack node type is re-exported as `PackNode`, not `Node` — the crate
// root already exports `projection::Node`. Inside `overview_pack.rs` it is still
// plain `Node`; only the external name is aliased.

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

/// Slots are 1-based and dense over the whole stream (blue-marshal's
/// store-before-ref encode order), so each `Shared` is defined at an earlier
/// unrelated key and referenced later by `Ref` — the shape real files use.
fn realish_user() -> Value {
    let overview = Value::Dict(vec![
        (Value::Int(900), Value::Shared { slot: 1, value: Box::new(Value::List(vec![Value::Int(9), Value::Int(13)])) }),
        (Value::Int(901), Value::Shared { slot: 2, value: Box::new(b("background")) }),
        (Value::Int(902), Value::Shared { slot: 3, value: Box::new(b("groups")) }),
        (b("backgroundStates2"), Value::Tuple(vec![ts(), Value::Ref(1)])),
        (b("overviewProfilePresets"), Value::Tuple(vec![ts(), Value::Dict(vec![
            (b("Friendly"), Value::Dict(vec![
                (Value::Ref(3), Value::List(vec![Value::Int(25)])),
            ])),
        ])])),
        (b("stateColors"), Value::Tuple(vec![ts(), Value::Dict(vec![
            (Value::Tuple(vec![Value::Ref(2), Value::Int(16)]),
             Value::Tuple(vec![Value::Float(0.0), Value::Float(0.15), Value::Float(0.6), Value::Float(1.0)])),
        ])])),
    ]);
    Value::Dict(vec![(b("overview"), overview)])
}

#[test]
fn reads_through_shared_and_ref() {
    // Round-trip through the codec so the tree is exactly what a file holds.
    let doc = decode(&encode(&realish_user()).unwrap()).unwrap();
    let (pack, _) = read_pack(&doc);

    let states = pack.get("backgroundStates").expect("state list read through a Ref");
    assert_eq!(settings_model::PackNode::Seq(vec![settings_model::PackNode::Int(9), settings_model::PackNode::Int(13)]), *states);
    assert!(pack.get("presets").is_some(), "preset field name read through a Ref");
    let colors = pack.get("stateColorsNameList").expect("colour surface read through a Ref");
    assert_eq!(format!("{colors:?}").contains("background_16"), true);

    // and it still emits valid YAML
    assert!(parse_pack(&emit_pack(&pack)).is_ok());
}
```

- [ ] **Step 6: Run the realshape test**

Run: `cargo test -p settings-model --test overview_pack_realshape`
Expected: PASS. If it fails while the unit tests pass, the read path is matching bare values somewhere — fix by routing that hop through `effective`.

- [ ] **Step 7: Export `read_pack` and commit**

Extend `crates/settings-model/src/lib.rs`:

```rust
pub use overview_pack::{color_name, color_rgba, emit_pack, parse_pack, read_pack, Node, Pack, PackError, SECTIONS};
```

```bash
git add crates/settings-model/src/overview_pack.rs crates/settings-model/src/lib.rs crates/settings-model/tests/overview_pack_realshape.rs
git commit -m "Read an overview pack out of an account file"
```

---

### Task 5: Apply a pack — everything except presets and tabs

**Files:**
- Modify: `crates/settings-model/src/overview_pack.rs`
- Modify: `crates/settings-model/src/lib.rs`

**Interfaces:**
- Consumes: `Pack`, `Node`, `color_rgba`, `split_surface_key`, `LIST_SECTIONS`, `USER_SETTINGS` (Tasks 1–4); `overview_tabs::{is_b, overview_mut, OverviewTabError}`; `treewalk::inline_all`.
- Produces:
  - `pub struct PackReport { pub applied: Vec<String>, pub warnings: Vec<String> }` (`#[derive(Serialize)]`, camelCase fields not needed — the frontend reads `applied`/`warnings`)
  - `pub fn apply_pack(v: &mut Value, pack: &Pack) -> Result<PackReport, PackError>`

Presets and tabs are Task 6; this task applies the other sections and leaves a `TODO`-free seam (the `presets`/`tabSetup` sections are simply not handled yet, and the test asserts that).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/settings-model/src/overview_pack.rs`:

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p settings-model overview_pack`
Expected: FAIL to compile — `cannot find function apply_pack`, `no variant NoOverview`.

- [ ] **Step 3: Write the implementation**

`PackError::NoOverview` already exists from Task 1. Add to `crates/settings-model/src/overview_pack.rs` (extend the imports with `use crate::overview_tabs::{is_b, overview_mut}; use crate::treewalk::inline_all;`):

```rust
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
            other => *other = value,
        },
        None => ov.push((
            Value::Bytes(key.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), value]),
        )),
    }
}

/// Columns are stored as a BARE list (no `(ts, _)` wrapper) on real files.
fn put_bare(ov: &mut Entries, key: &[u8], value: Value) {
    match ov.iter_mut().find(|(k, _)| is_b(k, key)) {
        Some((_, slot)) => *slot = value,
        None => ov.push((Value::Bytes(key.to_vec()), value)),
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
    let mut writes: Vec<(&[u8], Value, bool)> = Vec::new(); // (key, value, wrapped)

    for (section, key) in LIST_SECTIONS {
        let Some(node) = pack.get(section) else { continue };
        // Columns are stored BARE on real files; the four state lists are wrapped
        // in `(timestamp, list)`. (`key` is a `&[u8]`, so compare against slices —
        // a `b"…"` pattern is a `&[u8; N]` and will not typecheck here.)
        let wrapped = key != b"overviewColumnOrder".as_slice() && key != b"overviewColumns".as_slice();
        let value = if wrapped {
            let mut ids = ints(node);
            // Enabled lists are stored sorted and deduplicated; order lists keep
            // the pack's sequence (the slice-3 convention).
            if matches!(section, "backgroundStates" | "flagStates") {
                ids.sort_unstable();
                ids.dedup();
            }
            Value::List(ids.into_iter().map(Value::Int).collect())
        } else {
            Value::List(strs(node).into_iter().map(|s| Value::Bytes(s.into_bytes())).collect())
        };
        writes.push((key, value, wrapped));
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
        writes.push((b"stateColors", Value::Dict(entries), true));
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
        writes.push((b"stateBlinks", Value::Dict(entries), true));
        report.applied.push("stateBlinks".to_string());
    }

    // shipLabels: rebuild the file's ordered list of label dicts from the pack's
    // order list plus its name-keyed bodies. Field values are Bytes, as the file
    // stores them; `state` stays an int.
    if let (Some(order), Some(labels)) = (pack.get("shipLabelOrder"), pack.get("shipLabels")) {
        let bodies = pairs(labels);
        let Node::Seq(order_items) = order else { return Err(PackError::NotAPack) };
        let mut list = Vec::new();
        for want in order_items {
            let Some((_, body)) = bodies.iter().find(|(k, _)| *k == want) else { continue };
            let fields: Entries = pairs(body)
                .into_iter()
                .filter_map(|(k, val)| {
                    let key = Value::Bytes(as_str(k)?.as_bytes().to_vec());
                    let value = match val {
                        Node::Null => Value::None,
                        Node::Int(i) => Value::Int(*i),
                        Node::Bool(x) => Value::Bool(*x),
                        Node::Str(s) => Value::Bytes(s.clone().into_bytes()),
                        _ => return None,
                    };
                    Some((key, value))
                })
                .collect();
            list.push(Value::Dict(fields));
        }
        writes.push((b"shipLabels", Value::List(list), true));
        report.applied.push("shipLabels".to_string());
    }

    if let Some(node) = pack.get("userSettings") {
        for (k, val) in pairs(node) {
            let (Some(name), Node::Bool(on)) = (as_str(k), val) else { continue };
            match USER_SETTINGS.iter().find(|(pack_name, _)| *pack_name == name) {
                Some((_, file_key)) => {
                    debug_assert!(OVERVIEW_BOOLS.contains(file_key));
                    writes.push((file_key.as_bytes(), Value::Bool(*on), true));
                }
                None => report.warnings.push(format!("ignored unknown setting '{name}'")),
            }
        }
        report.applied.push("userSettings".to_string());
    }

    // --- mutate phase ---
    inline_all(v);
    let ov = overview_mut(v).map_err(|_| PackError::NoOverview)?;
    for (key, value, wrapped) in writes {
        if wrapped { put(ov, key, value) } else { put_bare(ov, key, value) }
    }
    Ok(report)
}
```

`OverviewTabError` is not exposed here: `overview_mut`'s only failure is a missing container, mapped to `PackError::NoOverview`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p settings-model overview_pack`
Expected: PASS (19 tests).

- [ ] **Step 5: Export and commit**

Extend `crates/settings-model/src/lib.rs`:

```rust
pub use overview_pack::{apply_pack, color_name, color_rgba, emit_pack, parse_pack, read_pack, Node, Pack, PackError, PackReport, SECTIONS};
```

```bash
git add crates/settings-model/src/overview_pack.rs crates/settings-model/src/lib.rs
git commit -m "Apply an overview pack's states, colours and columns"
```

---

### Task 6: Apply a pack — presets, tabs and the window mapping

Replacing `tabsettings_new` invalidates `tabsByWindowInstanceID`, whose entries are per-window lists of tab indices. Rule: assign every pack tab to the primary window (position 0) and drop from any secondary window every index the pack does not define. Never fabricate the mapping when it is absent.

New tab dicts are CLONED from an existing tab (the account's lowest-index tab), with the pack's `bracket`/`name`/`overview` overridden and per-tab column keys removed. Cloning is required: every real tab carries `bracket` and `color`, and EVE's "reset all overview settings" throws on a tab missing them.

**Files:**
- Modify: `crates/settings-model/src/overview_pack.rs`

**Interfaces:**
- Consumes: everything from Task 5; `overview_tabs::{dict_inner_mut, tabs_mut, as_int}`.
- Produces: no new public names — `apply_pack` gains the `presets` and `tabSetup` sections.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/settings-model/src/overview_pack.rs`:

```rust
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

    const TWO_TAB_PACK: &str = "presets:\n\
- - Enemies\n\
  - - - alwaysShownStates\n      - []\n    - - filteredStates\n      - []\n    - - groups\n      - - 27\n\
tabSetup:\n\
- - 0\n  - - - bracket\n      - Enemies\n    - - name\n      - Tab A\n    - - overview\n      - Enemies\n\
- - 1\n  - - - bracket\n      - Enemies\n    - - name\n      - Tab B\n    - - overview\n      - Enemies\n";

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
    fn rebuilds_the_window_mapping_without_dangling_indices() {
        let mut doc = user_doc_with_windows();
        apply_pack(&mut doc, &parse_pack(TWO_TAB_PACK).unwrap()).unwrap();

        let mut probe = doc.clone();
        let ov = crate::overview_tabs::overview_mut(&mut probe).unwrap();
        let (_, groups) = ov.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let text = format!("{groups:?}");
        assert!(text.contains("Int(0)") && text.contains("Int(1)"), "both pack tabs are mapped: {text}");
        assert!(!text.contains("Int(7)"), "the dangling index is gone: {text}");
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p settings-model overview_pack`
Expected: FAIL — `replaces_presets_and_tabs` fails because `presets`/`tabSetup` are not applied (the file's original preset and tab are still there).

- [ ] **Step 3: Write the implementation**

In `apply_pack`, insert this BEFORE the `// --- mutate phase ---` comment (it only computes; the tab work needs the tree, so it runs in the mutate phase below):

```rust
    // Presets: name-keyed dict of the three int lists. Built here, written below.
    let presets_value = pack.get("presets").map(|node| {
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
        Value::Dict(entries)
    });
```

and replace the mutate phase with:

```rust
    // --- mutate phase ---
    inline_all(v);
    let ov = overview_mut(v).map_err(|_| PackError::NoOverview)?;
    for (key, value, wrapped) in writes {
        if wrapped { put(ov, key, value) } else { put_bare(ov, key, value) }
    }

    if let Some(value) = presets_value {
        put(ov, b"overviewProfilePresets", value);
        report.applied.push("presets".to_string());
        // A stale unsaved working copy under a name the pack does not define
        // would resurrect a phantom preset, exactly as rename/delete guard
        // against in slice 2a.
        ov.retain(|(k, _)| !is_b(k, b"overviewProfilePresets_notSaved"));
    }

    if let Some(node) = pack.get("tabSetup") {
        apply_tabs(ov, node);
        report.applied.push("tabSetup".to_string());
    }

    Ok(report)
}

/// Replace the tab dict from a pack's `tabSetup`, then re-point the window
/// mapping. New tabs CLONE an existing tab so they keep the `bracket`/`color`
/// keys EVE's reset path reads; per-tab column overrides are dropped, because
/// pack columns are account-global.
fn apply_tabs(ov: &mut Entries, node: &Node) {
    use crate::overview_tabs::{as_int, dict_inner_mut, tabs_mut};

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
        let mut tab = template.clone().unwrap_or_else(|| Value::Dict(vec![
            (Value::Bytes(b"color".to_vec()), Value::None),
        ]));
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
    let Some((_, groups_val)) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")) else { return };
    let groups = match groups_val {
        Value::List(l) => l,
        Value::Tuple(items) => match items.iter_mut().find_map(|e| match e { Value::List(l) => Some(l), _ => None }) {
            Some(l) => l,
            None => return,
        },
        _ => return,
    };
    for (pos, window) in groups.iter_mut().enumerate() {
        let Some(list) = (match window {
            Value::List(l) => Some(l),
            Value::Tuple(items) => items.iter_mut().find_map(|e| match e { Value::List(l) => Some(l), _ => None }),
            _ => None,
        }) else { continue };
        if pos == 0 {
            *list = indices.iter().map(|i| Value::Int(*i)).collect();
        } else {
            list.retain(|e| matches!(e, Value::Int(i) if indices.contains(i)));
        }
    }
}
```

Note the closing brace placement: `apply_pack` now ends after `Ok(report)`, and `apply_tabs` is a sibling function.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p settings-model overview_pack`
Expected: PASS (23 tests). Then `cargo test -p settings-model` — expected: all green, including the realshape files.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/overview_pack.rs
git commit -m "Apply an overview pack's presets and tabs"
```

---

### Task 7: Commands

**Files:**
- Modify: `app/src-tauri/src/ops.rs`
- Modify: `app/src-tauri/src/lib.rs`
- Modify: `app/src/lib/api.ts`

**Interfaces:**
- Consumes: `settings_model::{apply_pack, emit_pack, parse_pack, read_pack, Pack, PackError, PackReport}`.
- Produces:
  - Rust: `pub fn pack_preview(path: &str) -> Result<PackSummary, ErrDto>`, `pub fn pack_import(state: &AppState, path: &str) -> Result<OverviewColumns, ErrDto>`, `pub fn pack_export(state: &AppState, path: &str) -> Result<PackReport, ErrDto>`; `pub struct PackSummary { pub sections: Vec<(String, usize)>, pub ignored: Vec<String> }`
  - Tauri commands `pack_preview`, `pack_import`, `pack_export`
  - TS: `api.packPreview(path)`, `api.packImport(path)`, `api.packExport(path)` with `PackSummary` / `PackReport` types

`pack_import` returns `OverviewColumns` like every other overview command, so the view re-projects from the same shape. Its report warnings are surfaced by the *preview* step (which already parsed the file) plus `pack_export`'s own return.

Deviation from the spec worth noting for review: §3.1 listed a `summarize(&Pack)` helper in the crate; the summary is two lines over `pack.sections` and lives in `ops::pack_preview` instead, so the crate keeps one fewer public name. Same behaviour.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module at the bottom of `app/src-tauri/src/ops.rs` (follow the existing helpers there — `open_file(&state, Slot::User, path)` on a temp file):

```rust
    /// A user file with one preset and one tab — enough for every pack section
    /// the command layer touches. Mirrors `user_doc()` in `overview_pack.rs`.
    fn pack_user_fixture() -> Value {
        let bb = |s: &str| Value::Bytes(s.as_bytes().to_vec());
        let ts = || Value::Long(vec![0u8; 8]);
        let preset = Value::Dict(vec![
            (bb("groups"), Value::List(vec![Value::Int(25)])),
            (bb("filteredStates"), Value::List(vec![])),
            (bb("alwaysShownStates"), Value::List(vec![])),
        ]);
        let tab = Value::Dict(vec![
            (bb("color"), Value::None),
            (bb("bracket"), bb("Friendly")),
            (bb("name"), Value::StrUcs2("Fleet".into())),
            (bb("overview"), bb("Friendly")),
        ]);
        Value::Dict(vec![(bb("overview"), Value::Dict(vec![
            (bb("overviewProfilePresets"), Value::Tuple(vec![ts(), Value::Dict(vec![(bb("Friendly"), preset)])])),
            (bb("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
            (bb("overviewColumns"), Value::List(vec![bb("NAME")])),
        ]))])
    }

    #[test]
    fn pack_round_trip_through_the_commands() {
        let upath = temp_file("pack-roundtrip", &encode(&pack_user_fixture()).unwrap());
        let dir = upath.parent().unwrap().to_path_buf();
        let state = AppState::new();
        open_file(&state, Slot::User, upath.to_str().unwrap()).unwrap();

        let out = dir.join("pack.yaml");
        let report = pack_export(&state, out.to_str().unwrap()).unwrap();
        assert!(report.applied.iter().any(|s| s == "presets"), "{report:?}");

        let summary = pack_preview(out.to_str().unwrap()).unwrap();
        assert!(summary.sections.iter().any(|(name, n)| name == "presets" && *n > 0));

        // Importing what we just exported leaves the same overview in place.
        let before = overview_columns(&state).unwrap();
        pack_import(&state, out.to_str().unwrap()).unwrap();
        let after = overview_columns(&state).unwrap();
        assert_eq!(after.tabs.len(), before.tabs.len());
        assert_eq!(after.presets, before.presets);
    }

    #[test]
    fn pack_preview_rejects_a_non_pack_file() {
        let junk = temp_file("pack-junk", b"").parent().unwrap().join("junk.yaml");
        fs::write(&junk, "some: mapping\n").unwrap();
        let err = pack_preview(junk.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "not_a_pack");
    }

    #[test]
    fn pack_import_without_an_open_account_errors() {
        let p = temp_file("pack-nodoc", b"").parent().unwrap().join("pack.yaml");
        fs::write(&p, "backgroundStates:\n- 9\n").unwrap();
        let state = AppState::new();
        let err = pack_import(&state, p.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "no_document");
    }
```

`temp_file(name, bytes)` is the module's existing helper: it makes a per-name temp directory and writes `core_user_5.dat` into it, so `path.parent()` is a scratch directory these tests can put YAML files in.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p app pack_`
Expected: FAIL to compile — `cannot find function pack_export`.

- [ ] **Step 3: Write the implementation**

Add to `app/src-tauri/src/ops.rs`, next to the other overview commands:

```rust
/// What a pack file contains, for the confirm dialog — section name and the
/// number of entries in it (0 for a scalar section).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PackSummary {
    pub sections: Vec<(String, usize)>,
    pub ignored: Vec<String>,
}

/// Map a `PackError` to a frontend `ErrDto`, carrying its `code` tag — the same
/// shape as `tab_err`, so the UI sees `not_a_pack` / `yaml` / `no_overview`
/// rather than one opaque code.
fn pack_err(e: settings_model::PackError) -> ErrDto {
    let jv = serde_json::to_value(&e).unwrap_or_default();
    ErrDto::new(
        jv.get("code").and_then(|c| c.as_str()).unwrap_or("pack"),
        e.to_string(),
    )
}

fn read_pack_file(path: &str) -> Result<settings_model::Pack, ErrDto> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ErrDto::new("io", format!("{path}: {e}")))?;
    settings_model::parse_pack(&text).map_err(pack_err)
}

/// Parse a pack and describe it. Reads the file only — no lock, no mutation.
pub fn pack_preview(path: &str) -> Result<PackSummary, ErrDto> {
    let pack = read_pack_file(path)?;
    let sections = pack
        .sections
        .iter()
        .map(|(name, node)| {
            // `PackNode`, not `Node` — the crate root's `Node` is the projection type.
            let count = match node { settings_model::PackNode::Seq(items) => items.len(), _ => 0 };
            (name.clone(), count)
        })
        .collect();
    Ok(PackSummary { sections, ignored: pack.ignored })
}

/// Apply a pack to the open account file. Marks the slot dirty like every other
/// editor — the user saves, and the normal backup chain applies.
pub fn pack_import(state: &AppState, path: &str) -> Result<OverviewColumns, ErrDto> {
    let pack = read_pack_file(path)?;
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        settings_model::apply_pack(&mut doc.value, &pack).map_err(pack_err)?;
        doc.value = blue_marshal::reshare(&doc.value);
    }
    overview_columns(state)
}

/// Write the open account's overview out as a pack. Exports the IN-MEMORY
/// document, so unsaved edits are included.
pub fn pack_export(state: &AppState, path: &str) -> Result<settings_model::PackReport, ErrDto> {
    let (pack, warnings) = {
        let guard = state.user.lock().unwrap();
        let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        settings_model::read_pack(&doc.value)
    };
    if pack.sections.is_empty() {
        return Err(ErrDto::new("pack", "this account file has no overview settings to export".to_string()));
    }
    let text = settings_model::emit_pack(&pack);
    std::fs::write(path, text).map_err(|e| ErrDto::new("io", format!("{path}: {e}")))?;
    Ok(settings_model::PackReport {
        applied: pack.sections.iter().map(|(name, _)| name.clone()).collect(),
        warnings,
    })
}
```

Add the commands in `app/src-tauri/src/lib.rs` beside `overview_set_bool`:

```rust
#[tauri::command]
fn pack_preview(path: String) -> Result<ops::PackSummary, ErrDto> {
    ops::pack_preview(&path)
}

#[tauri::command]
fn pack_import(state: tauri::State<'_, AppState>, path: String) -> Result<settings_model::OverviewColumns, ErrDto> {
    ops::pack_import(&state, &path)
}

#[tauri::command]
fn pack_export(state: tauri::State<'_, AppState>, path: String) -> Result<settings_model::PackReport, ErrDto> {
    ops::pack_export(&state, &path)
}
```

and register them in the `generate_handler!` list, after `overview_set_bool`:

```rust
            pack_preview, pack_import, pack_export,
```

Add to `app/src/lib/api.ts`, beside the other overview wrappers:

```ts
export type PackSummary = { sections: [string, number][]; ignored: string[] };
export type PackReport = { applied: string[]; warnings: string[] };
```

```ts
  packPreview: (path: string) => invoke<PackSummary>("pack_preview", { path }),
  packImport: (path: string) => invoke<OverviewColumns>("pack_import", { path }),
  packExport: (path: string) => invoke<PackReport>("pack_export", { path }),
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p app pack_`
Expected: PASS (3 tests). Then `cargo test` for the workspace — expected: all green.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add the pack preview, import and export commands"
```

---

### Task 8: Overview view buttons, corpus guard, changelog

**Files:**
- Modify: `app/src/lib/OverviewView.svelte`
- Create: `crates/settings-model/tests/overview_pack_corpus.rs`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `api.packPreview`, `api.packImport`, `api.packExport`, `onUserDirty` (Task 7 and the existing view props).
- Produces: no new exports.

- [ ] **Step 1: Write the corpus guard**

Create `crates/settings-model/tests/overview_pack_corpus.rs`:

```rust
//! Real-data guard: every corpus `core_user` file must project to a pack that
//! emits YAML which parses back to the same tree. This is the quoting, unicode
//! and markup check — the corpus holds accounts that imported published packs,
//! so it carries the awkward strings a hand-written fixture would not think of.
//! Skips silently when the corpus is not checked out.

use std::path::{Path, PathBuf};
use settings_model::{emit_pack, parse_pack, read_pack};

fn user_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() { user_files(&p, out); }
        else if p.file_name().map_or(false, |n| n.to_string_lossy().starts_with("core_user_")) {
            out.push(p);
        }
    }
}

#[test]
fn every_corpus_user_file_round_trips_as_a_pack() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    let mut files = Vec::new();
    user_files(&root, &mut files);
    if files.is_empty() { eprintln!("corpus not present, skipping"); return; }

    let mut checked = 0usize;
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = blue_marshal::decode(&bytes) else { continue };
        let (pack, _) = read_pack(&doc);
        if pack.sections.is_empty() { continue }
        let text = emit_pack(&pack);
        let again = parse_pack(&text).unwrap_or_else(|e| panic!("{}: emitted YAML did not parse: {e:?}", path.display()));
        assert_eq!(again.sections, pack.sections, "{}: round trip changed the pack", path.display());
        checked += 1;
    }
    assert!(checked > 0, "corpus present but no user file produced a pack");
}
```

`blue-marshal` must be a dev-dependency of `settings-model` for this test; it is already a normal dependency, so no manifest change is needed.

- [ ] **Step 2: Run it**

Run: `cargo test -p settings-model --test overview_pack_corpus`
Expected: PASS. A failure here names the file and the section — fix the emitter or the reader, not the test.

- [ ] **Step 3: Add the buttons**

In `app/src/lib/OverviewView.svelte`, add to the imports:

```ts
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
```

`@tauri-apps/api` is already a dependency (it is where `invoke` comes from), so this adds no package.

Add these handlers inside `<script>`, after the existing tab handlers:

```ts
  // Pack import/export is account-wide, so it lives in the view header rather
  // than inside one sub-tab. Import marks the slot dirty; the user still saves.
  let packBusy = $state(false);

  // EVE's own export lands in Documents/EVE/Overview, so start the picker there.
  // Best-effort: if the path can't be resolved the dialog just opens wherever it
  // last was.
  async function overviewFolder(): Promise<string | undefined> {
    try {
      return `${await documentDir()}EVE/Overview`;
    } catch {
      return undefined;
    }
  }

  async function importPack() {
    const picked = await openDialog({
      multiple: false,
      defaultPath: await overviewFolder(),
      filters: [{ name: "Overview pack", extensions: ["yaml", "yml"] }],
    });
    if (typeof picked !== "string") return;
    packBusy = true;
    try {
      const summary = await api.packPreview(picked);
      const what = summary.sections
        .map(([name, count]) => (count > 0 ? `${name} (${count})` : name))
        .join(", ");
      const ignored = summary.ignored.length
        ? `\n\nIgnored unknown sections: ${summary.ignored.join(", ")}`
        : "";
      const ok = await confirm(
        `This pack contains: ${what}.\n\nEach of those replaces your account's current overview settings. Per-tab column overrides are discarded.${ignored}`,
        { title: "Import overview pack", kind: "warning" },
      );
      if (!ok) return;
      data = await api.packImport(picked);
      onUserDirty();
      await message("Pack imported. Save to write it to the account file.", { title: "Import overview pack" });
    } catch (e) {
      await message(errMessage(e), { title: "Import failed", kind: "error" });
    } finally {
      packBusy = false;
    }
  }

  async function exportPack() {
    const picked = await saveDialog({
      defaultPath: "overview.yaml",
      filters: [{ name: "Overview pack", extensions: ["yaml"] }],
    });
    if (typeof picked !== "string") return;
    packBusy = true;
    try {
      const report = await api.packExport(picked);
      const warnings = report.warnings.length ? `\n\n${report.warnings.join("\n")}` : "";
      await message(`Exported ${report.applied.length} section(s).${warnings}`, { title: "Export overview pack" });
    } catch (e) {
      await message(errMessage(e), { title: "Export failed", kind: "error" });
    } finally {
      packBusy = false;
    }
  }
```

Add the buttons to the sub-tab row markup, so they sit beside Columns/Filters/Appearance:

```svelte
  <div class="subtabs" role="tablist">
    {#each ["Columns", "Filters", "Appearance"] as name}
      <button role="tab" aria-selected={sub === name} class:active={sub === name}
              onclick={() => (sub = name)}>{name}</button>
    {/each}
    <span class="pack-actions">
      <button onclick={importPack} disabled={packBusy} title="Replace this account's overview from an EVE overview pack">Import pack…</button>
      <button onclick={exportPack} disabled={packBusy} title="Write this account's overview out as an EVE overview pack">Export pack…</button>
    </span>
  </div>
```

and to the `<style>` block:

```css
  .pack-actions { margin-left: auto; display: flex; gap: 0.4rem; }
```

The `.subtabs` rule already lays its children out in a row; if it is not a flex container, add `display: flex; align-items: center;` to it so `margin-left: auto` pushes the pair to the right.

- [ ] **Step 4: Check the frontend**

Run: `cd app && npm run check`
Expected: no errors (0 errors, warnings unchanged from before the task).

Run: `cd app && npm test`
Expected: PASS — no new frontend tests here; the logic added is dialog plumbing, and the format logic is tested in Rust.

- [ ] **Step 5: Update the changelog**

Under `## [Unreleased]` in `CHANGELOG.md`:

```markdown
### Added
- **Import and export overview packs.** The Overview editor reads and writes the
  same YAML file EVE's own Overview Settings → Import/Export uses, so a
  downloaded community pack can be applied to an account without logging in, and
  your own overview can be shared as a pack EVE loads. Every section the pack
  defines — presets, tabs, columns, state colours and colortags, blink flags,
  in-space ship labels and the overview toggles — replaces that part of the
  account; sections the pack omits are left alone, so modular "preset only"
  packs work. Importing marks the file dirty: you still press Save, and the
  usual backup is taken.
```

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/OverviewView.svelte crates/settings-model/tests/overview_pack_corpus.rs CHANGELOG.md
git commit -m "Add pack import and export to the Overview editor"
```

- [ ] **Step 7: Full verification before review**

Run each and confirm green:

```bash
cargo test
cd app && npm run check && npm test && npm run build
```

Then the live smoke, per the spec's §8:
1. Import a published community pack; launch EVE; confirm tabs, presets, colours and in-space ship labels match.
2. Export from the editor; confirm EVE's own Import Overview Settings accepts the file.
3. Note which internal boolean `applyOnlyToShips` corresponds to, and whether a current-client export uses the suffixed or unsuffixed state-list names.
4. Repeat the import on an account with two overview windows and confirm the tab mapping.

Record anything surfaced as follow-up commits on this branch before the whole-branch review.
