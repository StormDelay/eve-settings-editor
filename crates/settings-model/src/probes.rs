//! Custom probe formations: `ui → probescanning.customFormations`, a
//! `(timestamp, {Int id: (name, List[((x, y, z), range)])})` — the saved
//! arrangements in the probe scanner's formation menu. Account-side.
//!
//! Three traps, all corpus-measured (docs/settings-field-reference.md, "Custom
//! probe formations"; placement in the editor's spec §2.1):
//!
//! - **The dot is part of the key name.** `probescanning.customFormations` is a
//!   single key under `ui`, not a path through a `probescanning` section.
//! - **Id `-4` is the client's scratch copy** of the formation being edited,
//!   not a user formation. Never projected, never written.
//! - **Its name is `Bytes` where every user formation's is `Str`**, so a reader
//!   matching one shape blanks on the other. `treewalk::text` handles both.
//!
//! A fourth trap is this module's own: a formation entry `(name, probes)` and
//! the `(FILETIME, value)` wrapper are BOTH 2-tuples. `wrapper_payload` checks
//! for the `Long` first element rather than the length, or every formation
//! unwraps into its probe list.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, find_child, section, text, SharedTable};

const KEY: &[u8] = b"probescanning.customFormations";
const SELECTED_KEY: &[u8] = b"probescanning.selectedFormationID";

/// 0.5 AU in metres. Every one of the 984 corpus probe entries carries exactly
/// this, to the metre.
pub const DEFAULT_RANGE: f64 = 74_798_935_350.0;

/// A formation holds 1 to 8 probes. The corpus only ever shows 8; the client
/// lets a player launch fewer, so shorter formations are accepted (spec §2.4).
pub const MAX_PROBES: usize = 8;

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ProbeError {
    /// No `ui` section in the document.
    NoUi,
    /// No `probescanning.customFormations` under `ui`.
    NoFormations,
    /// An id the file does not hold — including any negative id, which is never
    /// a user formation.
    NoSuchFormation,
    /// A write with no probes, or more than `MAX_PROBES`.
    BadProbeCount,
    /// A name that is empty once trimmed.
    BadName,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::NoUi => write!(f, "This file has no UI section."),
            ProbeError::NoFormations => write!(f, "This account has no custom probe formations."),
            ProbeError::NoSuchFormation => write!(f, "That probe formation no longer exists."),
            ProbeError::BadProbeCount => write!(f, "A formation needs between 1 and 8 probes."),
            ProbeError::BadName => write!(f, "A formation needs a name."),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Formation {
    pub id: i64,
    pub name: String,
    /// Metre offsets from the formation centre. EVE's axes: X and Z are the
    /// horizontal plane, Y is up.
    pub probes: Vec<[f64; 3]>,
    /// Metres, one per probe, positionally matching `probes` — the file's own
    /// values, kept whole. The editor writes ONE range back (spec §2.3), but
    /// reading has to stay faithful or a mixed formation could not be shown as
    /// what it is.
    pub ranges: Vec<f64>,
    /// Metres. The first probe's range — what the single range field edits.
    pub range: f64,
    /// The probes did not agree on a range. No corpus formation does this; the
    /// flag exists so the UI can show the file rather than flatten it.
    pub mixed_range: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Formations {
    /// Ascending by id, negative ids excluded.
    pub formations: Vec<Formation>,
    /// `probescanning.selectedFormationID`, when the file has it.
    pub selected: Option<i64>,
}

/// The value inside a `(FILETIME, value)` wrapper, or the value itself.
///
/// The `Long` guard is load-bearing: a formation entry is also a 2-tuple, so a
/// bare length check would unwrap `(name, probes)` into `probes`.
fn wrapper_payload<'a>(v: &'a Value, sh: &SharedTable<'a>) -> &'a Value {
    match effective(v, sh) {
        Value::Tuple(t) if t.len() == 2 && matches!(effective(&t[0], sh), Value::Long(_)) => {
            effective(&t[1], sh)
        }
        other => other,
    }
}

fn float(v: &Value, sh: &SharedTable) -> Option<f64> {
    match effective(v, sh) {
        Value::Float(f) => Some(*f),
        // Not seen in the corpus, but a whole-number coordinate could encode as
        // an Int, and refusing it would drop the whole formation.
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

fn read_probe(v: &Value, sh: &SharedTable) -> Option<([f64; 3], f64)> {
    let Value::Tuple(t) = effective(v, sh) else { return None };
    if t.len() != 2 {
        return None;
    }
    let Value::Tuple(xyz) = effective(&t[0], sh) else { return None };
    if xyz.len() != 3 {
        return None;
    }
    let pos = [float(&xyz[0], sh)?, float(&xyz[1], sh)?, float(&xyz[2], sh)?];
    Some((pos, float(&t[1], sh)?))
}

fn read_formation(id: i64, v: &Value, sh: &SharedTable) -> Option<Formation> {
    let Value::Tuple(t) = effective(v, sh) else { return None };
    if t.len() != 2 {
        return None;
    }
    let name = text(&t[0], sh)?;
    let list = match effective(&t[1], sh) {
        Value::List(l) | Value::Tuple(l) => l,
        _ => return None,
    };
    let read: Vec<([f64; 3], f64)> = list.iter().filter_map(|p| read_probe(p, sh)).collect();
    // A partially-read formation is worse than none: the editor would save back
    // the probes it understood and silently drop the rest.
    if read.len() != list.len() || read.is_empty() {
        return None;
    }
    let range = read[0].1;
    Some(Formation {
        id,
        name,
        mixed_range: read.iter().any(|(_, r)| *r != range),
        probes: read.iter().map(|(p, _)| *p).collect(),
        ranges: read.iter().map(|(_, r)| *r).collect(),
        range,
    })
}

pub fn project_formations(v: &Value) -> Result<Formations, ProbeError> {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    // `section` resolves a Shared/Ref section key — account files store root
    // `ui` as a Ref, which a bare is_bytes match misses (neocom.rs documents
    // the same trap).
    let (entries, _) = section(v, b"ui", &sh).ok_or(ProbeError::NoUi)?;
    let raw = find_child(entries, KEY, &sh).ok_or(ProbeError::NoFormations)?;
    let Value::Dict(d) = wrapper_payload(raw, &sh) else { return Err(ProbeError::NoFormations) };
    let mut formations: Vec<Formation> = d
        .iter()
        .filter_map(|(k, val)| {
            let Value::Int(id) = effective(k, &sh) else { return None };
            // The -4 scratch slot and anything else negative.
            if *id < 0 {
                return None;
            }
            read_formation(*id, val, &sh)
        })
        .collect();
    formations.sort_by_key(|f| f.id);
    let selected = find_child(entries, SELECTED_KEY, &sh).and_then(|v| {
        match wrapper_payload(v, &sh) {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    });
    Ok(Formations { formations, selected })
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    fn f(x: f64) -> Value { Value::Float(x) }

    /// One probe entry: ((x, y, z), range).
    fn probe(x: f64, y: f64, z: f64, r: f64) -> Value {
        Value::Tuple(vec![Value::Tuple(vec![f(x), f(y), f(z)]), f(r)])
    }

    /// A formation entry: (name, [probe, ...]).
    fn formation(name: Value, probes: Vec<Value>) -> Value {
        Value::Tuple(vec![name, Value::List(probes)])
    }

    /// user -> ui -> { customFormations: (ts, dict), selectedFormationID: (ts, Int) }
    ///
    /// Holds the two ids the corpus shows (0 "close", 1 "on grid") plus the -4
    /// scratch slot, whose name is Bytes where the others are Str.
    fn doc() -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("close".into()), vec![
                    probe(-1199120384.0, -115136512.0, -415997952.0, DEFAULT_RANGE),
                    probe(22762389504.0, -115136512.0, -122200064.0, DEFAULT_RANGE),
                ])),
                (Value::Int(1), formation(Value::Str("on grid".into()), vec![
                    probe(1.0, 2.0, 3.0, DEFAULT_RANGE),
                ])),
                (Value::Int(-4), formation(b("tempFormation"), vec![
                    probe(9.0, 9.0, 9.0, DEFAULT_RANGE),
                ])),
            ])])),
            (b("probescanning.selectedFormationID"), Value::Tuple(vec![ts(), Value::Int(0)])),
        ]))])
    }

    #[test]
    fn projects_user_formations_in_id_order() {
        let p = project_formations(&doc()).expect("projects");
        assert_eq!(p.formations.len(), 2, "the -4 scratch slot must not be projected");
        assert_eq!(p.formations[0].id, 0);
        assert_eq!(p.formations[0].name, "close");
        assert_eq!(p.formations[0].probes.len(), 2);
        assert_eq!(p.formations[1].id, 1);
        assert_eq!(p.formations[1].name, "on grid");
        assert_eq!(p.selected, Some(0));
    }

    #[test]
    fn coordinates_survive_the_projection_exactly() {
        // These are f64 read straight off the wire. Any rounding here would
        // displace every probe in the file the moment the editor saved.
        let p = project_formations(&doc()).unwrap();
        assert_eq!(p.formations[0].probes[0], [-1199120384.0, -115136512.0, -415997952.0]);
        assert_eq!(p.formations[0].range, DEFAULT_RANGE);
        assert!(!p.formations[0].mixed_range);
    }

    #[test]
    fn a_bytes_name_reads_as_text() {
        // The scratch slot's name is Bytes where user formations are Str. It is
        // not projected, but `text` handling both shapes is what keeps a reader
        // that meets one in a user slot from blanking. Assert on the helper.
        let mut sh = SharedTable::new();
        let v = b("tempFormation");
        collect_shared(&v, &mut sh);
        assert_eq!(text(&v, &sh).as_deref(), Some("tempFormation"));
    }

    #[test]
    fn a_mixed_range_formation_is_flagged_not_flattened() {
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("odd".into()), vec![
                    probe(1.0, 0.0, 0.0, DEFAULT_RANGE),
                    probe(2.0, 0.0, 0.0, DEFAULT_RANGE / 2.0),
                ])),
            ])])),
        ]))]);
        let p = project_formations(&d).unwrap();
        assert!(p.formations[0].mixed_range, "differing ranges must be reported");
        assert_eq!(p.formations[0].range, DEFAULT_RANGE, "range reports the first entry");
        assert_eq!(
            p.formations[0].ranges,
            vec![DEFAULT_RANGE, DEFAULT_RANGE / 2.0],
            "the per-probe ranges must survive whole, so the view can name which rows differ",
        );
    }

    #[test]
    fn a_formation_entry_is_not_mistaken_for_a_timestamp_wrapper() {
        // Both `(FILETIME, dict)` and `(name, probes)` are 2-tuples. A wrapper
        // unwrapper that only checks the length turns every formation into its
        // probe list and projects nothing.
        let p = project_formations(&doc()).unwrap();
        assert_eq!(p.formations[0].name, "close", "the name half must survive");
    }

    #[test]
    fn a_file_without_the_key_reports_no_formations() {
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("other"), Value::Int(1))]))]);
        assert_eq!(project_formations(&d), Err(ProbeError::NoFormations));
    }

    #[test]
    fn a_file_without_ui_reports_no_ui() {
        let d = Value::Dict(vec![(b("windows"), Value::Dict(Vec::new()))]);
        assert_eq!(project_formations(&d), Err(ProbeError::NoUi));
    }

    #[test]
    fn an_absent_selected_id_is_none() {
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("a".into()), vec![probe(1.0, 0.0, 0.0, DEFAULT_RANGE)])),
            ])])),
        ]))]);
        assert_eq!(project_formations(&d).unwrap().selected, None);
    }
}
