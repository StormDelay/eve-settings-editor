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

use crate::treewalk::{
    collect_shared, effective, find_child, inline_all, is_bytes, section, text, SharedTable,
};

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
    /// A write whose range count does not match its probe count.
    BadRangeCount,
    /// A name that is empty once trimmed.
    BadName,
    /// Shared text that is not valid YAML, or valid YAML in a shape a
    /// formation cannot be read out of.
    BadYaml { message: String },
    /// Valid YAML with no top-level `formations:` list — the user picked the
    /// wrong file (sharing spec §2.4).
    NotFormations,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::NoUi => write!(f, "This file has no UI section."),
            ProbeError::NoFormations => write!(f, "This account has no custom probe formations."),
            ProbeError::NoSuchFormation => write!(f, "That probe formation no longer exists."),
            ProbeError::BadProbeCount => write!(f, "A formation needs between 1 and 8 probes."),
            ProbeError::BadRangeCount => write!(f, "Every probe needs a scan range."),
            ProbeError::BadName => write!(f, "A formation needs a name."),
            ProbeError::BadYaml { message } => {
                write!(f, "This is not a readable formation file: {message}")
            }
            ProbeError::NotFormations => write!(f, "This file contains no probe formations."),
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
    /// Metres, one per probe, positionally matching `probes`. The format
    /// carries one range per entry because the client sets scan range per
    /// probe; every corpus entry agreeing on 0.5 AU is a fact about players,
    /// not about the format (spec §2.1).
    pub ranges: Vec<f64>,
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
    Some(Formation {
        id,
        name,
        probes: read.iter().map(|(p, _)| *p).collect(),
        ranges: read.iter().map(|(_, r)| *r).collect(),
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

/// The formations dict, mutable, minting a `(zero Long, Dict)` wrapper when the
/// key is absent — the `overview_states.rs` rule. EVE re-stamps on its next
/// save. The document must already be inlined, so no `Shared` survives here.
fn formations_mut(v: &mut Value) -> Result<&mut Vec<(Value, Value)>, ProbeError> {
    let Value::Dict(top) = v else { return Err(ProbeError::NoUi) };
    let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).ok_or(ProbeError::NoUi)?;
    let Value::Dict(entries) = ui else { return Err(ProbeError::NoUi) };
    if !entries.iter().any(|(k, _)| is_bytes(k, KEY)) {
        entries.push((
            Value::Bytes(KEY.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, raw) = entries
        .iter_mut()
        .find(|(k, _)| is_bytes(k, KEY))
        .expect("just ensured present");
    // The length check is split out of the match so the arm below is an
    // unguarded reborrow — a guarded arm does not borrow-check against a
    // move-catch-all on a `&mut` scrutinee (the neocom.rs note).
    let wrapped = matches!(raw, Value::Tuple(t) if t.len() == 2 && matches!(t[0], Value::Long(_)));
    let payload = if wrapped {
        let Value::Tuple(t) = raw else { unreachable!() };
        &mut t[1]
    } else {
        raw
    };
    match payload {
        Value::Dict(d) => Ok(d),
        _ => Err(ProbeError::NoFormations),
    }
}

/// Point `selectedFormationID` at `id`, preserving an existing stamp and
/// minting a zero one when the key is absent. The document must be inlined.
fn set_selected(v: &mut Value, id: i64) -> Result<(), ProbeError> {
    let Value::Dict(top) = v else { return Err(ProbeError::NoUi) };
    let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).ok_or(ProbeError::NoUi)?;
    let Value::Dict(entries) = ui else { return Err(ProbeError::NoUi) };
    match entries.iter_mut().find(|(k, _)| is_bytes(k, SELECTED_KEY)) {
        Some((_, slot)) => match slot {
            // Replace whichever element is not the stamp, rather than assuming
            // position — hunting for the Long and taking "the other one" is the
            // overview_states.rs idiom, and it cannot produce a malformed
            // (Long, Int, Int) the way pushing would.
            Value::Tuple(items) => match items.iter_mut().find(|e| !matches!(e, Value::Long(_))) {
                Some(inner) => *inner = Value::Int(id),
                None => *slot = Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Int(id)]),
            },
            other => *other = Value::Int(id),
        },
        None => entries.push((
            Value::Bytes(SELECTED_KEY.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Int(id)]),
        )),
    }
    Ok(())
}

/// The rules a formation must satisfy before anything is written. Split out of
/// `set_formation` so a BATCH can check every member before the first one is
/// inlined — otherwise a bad entry halfway down an import leaves half of it
/// applied (sharing spec §4.2).
pub fn check_formation(name: &str, probes: &[[f64; 3]], ranges: &[f64]) -> Result<(), ProbeError> {
    if name.trim().is_empty() {
        return Err(ProbeError::BadName);
    }
    if probes.is_empty() || probes.len() > MAX_PROBES {
        return Err(ProbeError::BadProbeCount);
    }
    if ranges.len() != probes.len() {
        return Err(ProbeError::BadRangeCount);
    }
    Ok(())
}

/// Replace the formation at `id`, or create it there.
///
/// `ranges` is one scan range per probe, positionally matching `probes` — the
/// format carries one per entry and the client sets one per probe (spec §2.1).
pub fn set_formation(
    v: &mut Value,
    id: i64,
    name: &str,
    probes: &[[f64; 3]],
    ranges: &[f64],
) -> Result<(), ProbeError> {
    // Validate BEFORE inlining, so a rejected write leaves the document
    // byte-for-byte as it was (the tests assert exactly this).
    if id < 0 {
        return Err(ProbeError::NoSuchFormation); // never the -4 scratch slot
    }
    check_formation(name, probes, ranges)?;
    inline_all(v);
    let d = formations_mut(v)?;
    let entry = Value::Tuple(vec![
        Value::Str(name.to_string()),
        Value::List(
            probes
                .iter()
                .zip(ranges)
                .map(|(p, r)| {
                    Value::Tuple(vec![
                        Value::Tuple(vec![Value::Float(p[0]), Value::Float(p[1]), Value::Float(p[2])]),
                        Value::Float(*r),
                    ])
                })
                .collect(),
        ),
    ]);
    match d.iter_mut().find(|(k, _)| matches!(k, Value::Int(i) if *i == id)) {
        Some((_, slot)) => *slot = entry,
        None => d.push((Value::Int(id), entry)),
    }
    Ok(())
}

/// Delete a formation, repointing `selectedFormationID` when it named this one.
/// Leaving the selection on a deleted formation is the one outcome that could
/// confuse the client.
pub fn remove_formation(v: &mut Value, id: i64) -> Result<(), ProbeError> {
    if id < 0 {
        return Err(ProbeError::NoSuchFormation);
    }
    let before = project_formations(v)?;
    if !before.formations.iter().any(|f| f.id == id) {
        return Err(ProbeError::NoSuchFormation);
    }
    inline_all(v);
    {
        let d = formations_mut(v)?;
        d.retain(|(k, _)| !matches!(k, Value::Int(i) if *i == id));
    }
    if before.selected == Some(id) {
        if let Some(next) = before.formations.iter().map(|f| f.id).filter(|i| *i != id).min() {
            set_selected(v, next)?;
        }
    }
    Ok(())
}

/// The smallest unused id `>= 0`. Corpus ids are small and reused rather than
/// minted, so this fills gaps rather than counting up from the maximum.
pub fn next_id(f: &Formations) -> i64 {
    (0i64..).find(|c| !f.formations.iter().any(|x| x.id == *c)).expect("i64 has a free id")
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
        assert_eq!(p.formations[0].ranges[0], DEFAULT_RANGE);
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
    fn per_probe_ranges_round_trip() {
        // The client sets scan range per probe. A corpus that only ever shows a
        // uniform range says how players use the control, not what it permits
        // (spec §2.1) — so a mixed formation is ordinary data, not a file to
        // lock read-only.
        let d = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("probescanning.customFormations"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (Value::Int(0), formation(Value::Str("odd".into()), vec![
                    probe(1.0, 0.0, 0.0, DEFAULT_RANGE),
                    probe(2.0, 0.0, 0.0, DEFAULT_RANGE / 2.0),
                ])),
            ])])),
        ]))]);
        let p = project_formations(&d).unwrap();
        assert_eq!(p.formations[0].ranges, vec![DEFAULT_RANGE, DEFAULT_RANGE / 2.0]);
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

    /// The formations dict as stored, for asserting on raw shape.
    fn stored(v: &Value) -> &Vec<(Value, Value)> {
        let Value::Dict(top) = v else { panic!("not a dict") };
        let (_, ui) = top.iter().find(|(k, _)| is_bytes(k, b"ui")).expect("ui");
        let Value::Dict(entries) = ui else { panic!("ui is not a dict") };
        let (_, raw) = entries.iter().find(|(k, _)| is_bytes(k, KEY)).expect("the key");
        let Value::Tuple(t) = raw else { panic!("not wrapped") };
        let Value::Dict(d) = &t[1] else { panic!("payload is not a dict") };
        d
    }

    /// The `(timestamp, _)` stamp on the formations key, for wrapper assertions.
    fn stamp(v: &Value) -> Value {
        let Value::Dict(top) = v else { panic!("not a dict") };
        let (_, ui) = top.iter().find(|(k, _)| is_bytes(k, b"ui")).expect("ui");
        let Value::Dict(entries) = ui else { panic!("ui is not a dict") };
        let (_, raw) = entries.iter().find(|(k, _)| is_bytes(k, KEY)).expect("the key");
        let Value::Tuple(t) = raw else { panic!("not wrapped") };
        t[0].clone()
    }

    fn seeded() -> Value {
        // A distinguishable non-zero stamp, so a test can tell "the original
        // survived" from "a fresh zero one was minted".
        let mut d = doc();
        let Value::Dict(top) = &mut d else { unreachable!() };
        let (_, ui) = top.iter_mut().find(|(k, _)| is_bytes(k, b"ui")).unwrap();
        let Value::Dict(entries) = ui else { unreachable!() };
        let (_, raw) = entries.iter_mut().find(|(k, _)| is_bytes(k, KEY)).unwrap();
        let Value::Tuple(t) = raw else { unreachable!() };
        t[0] = Value::Long(vec![7, 0, 0, 0, 0, 0, 0, 0]);
        d
    }

    #[test]
    fn set_replaces_an_existing_formation_and_keeps_the_stamp() {
        let mut v = seeded();
        set_formation(&mut v, 0, "closer", &[[1.0, 2.0, 3.0]], &[DEFAULT_RANGE]).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations[0].name, "closer");
        assert_eq!(p.formations[0].probes, vec![[1.0, 2.0, 3.0]]);
        assert_eq!(
            stamp(&v),
            Value::Long(vec![7, 0, 0, 0, 0, 0, 0, 0]),
            "the ORIGINAL timestamp must survive the edit, not be replaced",
        );
    }

    #[test]
    fn set_creates_a_formation_at_a_new_id() {
        let mut v = doc();
        set_formation(&mut v, 2, "new", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations.len(), 3);
        assert_eq!(p.formations[2].id, 2);
        assert_eq!(p.formations[2].name, "new");
    }

    #[test]
    fn a_written_name_is_str_never_bytes() {
        // The only Bytes name in the corpus is the scratch slot. Writing Bytes
        // would make a user formation look like one.
        let mut v = doc();
        set_formation(&mut v, 0, "close", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]).unwrap();
        let d = stored(&v);
        let (_, entry) = d.iter().find(|(k, _)| matches!(k, Value::Int(0))).unwrap();
        let Value::Tuple(t) = entry else { panic!("not a formation tuple") };
        assert_eq!(t[0], Value::Str("close".into()));
    }

    #[test]
    fn a_uniform_range_is_written_to_every_probe() {
        let mut v = doc();
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        set_formation(&mut v, 0, "even", &probes, &[123.0; 3]).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations[0].ranges, vec![123.0; 3]);
        assert_eq!(p.formations[0].probes.len(), 3);
    }

    #[test]
    fn each_probe_keeps_its_own_written_range() {
        let mut v = doc();
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        set_formation(&mut v, 0, "spread", &probes, &[100.0, 200.0, 300.0]).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations[0].ranges, vec![100.0, 200.0, 300.0]);
    }

    #[test]
    fn a_range_count_that_does_not_match_the_probes_is_rejected() {
        let before = doc();
        let mut v = doc();
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        assert_eq!(
            set_formation(&mut v, 0, "x", &probes, &[100.0]),
            Err(ProbeError::BadRangeCount),
        );
        assert_eq!(
            set_formation(&mut v, 0, "x", &probes, &[100.0, 200.0, 300.0]),
            Err(ProbeError::BadRangeCount),
        );
        assert_eq!(v, before, "a rejected write must not inline or otherwise touch the document");
    }

    #[test]
    fn the_scratch_slot_survives_a_write() {
        let mut v = doc();
        set_formation(&mut v, 0, "close", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]).unwrap();
        let d = stored(&v);
        assert!(
            d.iter().any(|(k, _)| matches!(k, Value::Int(-4))),
            "the client's -4 scratch slot must be left untouched",
        );
    }

    #[test]
    fn a_key_absent_from_the_file_is_minted_wrapped() {
        let mut v = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("other"), Value::Int(1))]))]);
        set_formation(&mut v, 0, "first", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]).unwrap();
        assert_eq!(
            stamp(&v),
            Value::Long(vec![0u8; 8]),
            "a freshly minted key must carry a zero Long stamp, not a bare dict",
        );
        assert_eq!(project_formations(&v).unwrap().formations.len(), 1);
    }

    #[test]
    fn a_rejected_write_leaves_the_document_untouched() {
        let before = doc();
        let mut v = doc();
        assert_eq!(set_formation(&mut v, 0, "x", &[], &[]), Err(ProbeError::BadProbeCount));
        assert_eq!(set_formation(&mut v, 0, "  ", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]), Err(ProbeError::BadName));
        let nine = [[1.0, 0.0, 0.0]; 9];
        assert_eq!(set_formation(&mut v, 0, "x", &nine, &[DEFAULT_RANGE; 9]), Err(ProbeError::BadProbeCount));
        assert_eq!(set_formation(&mut v, -4, "x", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]), Err(ProbeError::NoSuchFormation));
        assert_eq!(v, before, "a rejected write must not inline or otherwise touch the document");
    }

    #[test]
    fn one_and_eight_probes_are_both_accepted() {
        let mut v = doc();
        set_formation(&mut v, 0, "one", &[[1.0, 0.0, 0.0]], &[DEFAULT_RANGE]).unwrap();
        assert_eq!(project_formations(&v).unwrap().formations[0].probes.len(), 1);
        let eight = [[1.0, 0.0, 0.0]; 8];
        set_formation(&mut v, 0, "eight", &eight, &[DEFAULT_RANGE; 8]).unwrap();
        assert_eq!(project_formations(&v).unwrap().formations[0].probes.len(), 8);
    }

    #[test]
    fn remove_drops_the_formation() {
        let mut v = doc();
        remove_formation(&mut v, 1).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.formations.len(), 1);
        assert_eq!(p.formations[0].id, 0);
    }

    #[test]
    fn removing_the_selected_formation_repoints_the_selection() {
        let mut v = doc(); // selected = 0
        remove_formation(&mut v, 0).unwrap();
        let p = project_formations(&v).unwrap();
        assert_eq!(p.selected, Some(1), "the selection must never name a deleted formation");
    }

    #[test]
    fn removing_an_unselected_formation_leaves_the_selection_alone() {
        let mut v = doc(); // selected = 0
        remove_formation(&mut v, 1).unwrap();
        assert_eq!(project_formations(&v).unwrap().selected, Some(0));
    }

    #[test]
    fn removing_the_last_formation_leaves_the_selection_alone() {
        let mut v = doc();
        remove_formation(&mut v, 1).unwrap();
        remove_formation(&mut v, 0).unwrap();
        let p = project_formations(&v).unwrap();
        assert!(p.formations.is_empty());
        assert_eq!(p.selected, Some(0), "nothing to repoint at, so leave the key as it was");
    }

    #[test]
    fn remove_refuses_an_unknown_or_negative_id() {
        let mut v = doc();
        assert_eq!(remove_formation(&mut v, 9), Err(ProbeError::NoSuchFormation));
        assert_eq!(remove_formation(&mut v, -4), Err(ProbeError::NoSuchFormation));
        assert!(stored(&v).iter().any(|(k, _)| matches!(k, Value::Int(-4))));
    }

    #[test]
    fn next_id_fills_the_lowest_gap() {
        let p = project_formations(&doc()).unwrap(); // ids 0 and 1
        assert_eq!(next_id(&p), 2);
        let empty = Formations { formations: Vec::new(), selected: None };
        assert_eq!(next_id(&empty), 0);
        let gapped = Formations {
            formations: vec![
                Formation { id: 0, name: "a".into(), probes: vec![[0.0; 3]], ranges: vec![1.0] },
                Formation { id: 2, name: "b".into(), probes: vec![[0.0; 3]], ranges: vec![1.0] },
            ],
            selected: None,
        };
        assert_eq!(next_id(&gapped), 1, "ids are reused in the corpus, not counted upward");
    }
}
