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
