//! The probe-formation exchange format: the YAML text behind Copy/Paste and
//! Export/Import. `probes.rs` speaks the marshal document; this module speaks
//! YAML, and all format knowledge stays on this side of that line — the same
//! split `overview_pack.rs` has from `overview_states.rs`.
//!
//! METRES, EXACTLY AS STORED. One metre is 6.7e-12 AU, so a format that
//! converted units would displace every probe of every formation that
//! round-trips through it — the editor spec's §4.2 argument, applied to text
//! that leaves the machine. Legibility comes from COMMENTS instead: an AU
//! reading beside the range, a kilometre distance beside each probe. The
//! parser skips them.
//!
//! NO IDS. An id is the account-local key of the `customFormations` dict and is
//! reused rather than minted, so an import allocates a fresh one (sharing spec
//! §2.2).

use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::probes::ProbeError;

/// EVE's own astronomical unit, for the emitted comments only — never for a
/// value that gets read back.
const M_PER_AU: f64 = 149_597_870_700.0;

/// A formation as it travels between files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormationSpec {
    pub name: String,
    /// Metre offsets from the formation centre. EVE's axes: X and Z are the
    /// horizontal plane, Y is up.
    pub probes: Vec<[f64; 3]>,
    /// Metres, one per probe, positionally matching `probes`.
    pub ranges: Vec<f64>,
}

/// A number that parses back to the same `f64`.
///
/// Integral values are written without a decimal point: every corpus
/// coordinate is one, and `-1199120384` reads where `-1199120384.0` only adds
/// noise across twenty-four of them. `i64` is exact well past 2^53, far beyond
/// any coordinate EVE stores. Everything else falls back to `{:?}`, Rust's
/// shortest round-tripping form and what `emit_pack` already uses for floats.
fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        return format!("{}", v as i64);
    }
    format!("{v:?}")
}

/// A range in AU, for the comment beside it. Six places is plenty: the nine
/// slider stops are all clean, and this is decoration, not data.
fn au(metres: f64) -> String {
    let s = format!("{:.6}", metres / M_PER_AU);
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-" { "0".to_string() } else { t.to_string() }
}

/// A YAML single-quoted scalar. Doubling the quote is the single-quoted style's
/// only escape, which is why this style is used — `emit_pack` writes names the
/// same way.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Render formations as the shared YAML text.
pub fn emit_formations(specs: &[FormationSpec]) -> String {
    let mut out = String::from(
        "# EVE probe formations. Positions and ranges are metres from the formation centre.\n",
    );
    out.push_str("formations:\n");
    for s in specs {
        out.push_str(&format!("  - name: {}\n", quote(&s.name)));
        let first = s.ranges.first().copied().unwrap_or(0.0);
        // Written even in the mixed case, so a reader that only understands
        // `range:` gets the first probe's value rather than nothing (§2.3).
        out.push_str(&format!("    range: {}          # {} AU\n", num(first), au(first)));
        if s.ranges.iter().any(|r| *r != first) {
            let list: Vec<String> = s.ranges.iter().map(|r| num(*r)).collect();
            out.push_str(&format!("    ranges: [{}]\n", list.join(", ")));
        }
        out.push_str("    probes:\n");
        // Column-align the coordinates. The point of metres is that they are
        // exact; a ragged block of eleven-digit numbers is unreadable, and
        // legibility is this format's other job.
        let cells: Vec<[String; 3]> =
            s.probes.iter().map(|p| [num(p[0]), num(p[1]), num(p[2])]).collect();
        let width = |i: usize| cells.iter().map(|c| c[i].len()).max().unwrap_or(0);
        let (w0, w1, w2) = (width(0), width(1), width(2));
        for (p, c) in s.probes.iter().zip(&cells) {
            let km = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() / 1000.0;
            out.push_str(&format!(
                "      - [{:>w0$}, {:>w1$}, {:>w2$}]   # {} km\n",
                c[0], c[1], c[2], km.round(),
            ));
        }
    }
    out
}

fn bad(what: impl Into<String>) -> ProbeError {
    ProbeError::BadYaml { message: what.into() }
}

/// A YAML scalar as a number. `Real` carries the source text; `String` covers a
/// quoted or exponent-formatted value a hand edit might produce, and refusing
/// those would fail a file the user could not see anything wrong with.
fn number(y: &Yaml) -> Option<f64> {
    match y {
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::Real(s) | Yaml::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn read_spec(y: &Yaml) -> Result<FormationSpec, ProbeError> {
    let Yaml::Hash(h) = y else { return Err(bad("a formations entry is not a mapping")) };
    let get = |k: &str| h.get(&Yaml::String(k.to_string()));
    let name = match get("name") {
        Some(Yaml::String(s)) => s.clone(),
        // `name: 2026` is a YAML integer, and a formation named after a year is
        // ordinary. Refusing it would be a puzzle, not a diagnosis.
        Some(Yaml::Integer(i)) => i.to_string(),
        _ => return Err(bad("a formation has no name")),
    };
    let Some(Yaml::Array(rows)) = get("probes") else {
        return Err(bad(format!("formation '{name}' has no probes list")));
    };
    let mut probes = Vec::with_capacity(rows.len());
    for row in rows {
        let Yaml::Array(xyz) = row else {
            return Err(bad(format!("a probe of '{name}' is not an [x, y, z] list")));
        };
        if xyz.len() != 3 {
            return Err(bad(format!("a probe of '{name}' does not have three coordinates")));
        }
        let mut p = [0.0f64; 3];
        for (slot, cell) in p.iter_mut().zip(xyz) {
            *slot = number(cell)
                .ok_or_else(|| bad(format!("a coordinate of '{name}' is not a number")))?;
        }
        probes.push(p);
    }
    // `ranges` wins: it is the only shape that survives a formation whose
    // probes disagree, and flattening one is the outcome §2.3 forbids.
    let ranges = match get("ranges") {
        Some(Yaml::Array(rs)) => rs
            .iter()
            .map(|r| number(r).ok_or_else(|| bad(format!("a range of '{name}' is not a number"))))
            .collect::<Result<Vec<f64>, ProbeError>>()?,
        // Present but not a list: NOT a fallback to `range:` — that would
        // silently discard whatever the user put in `ranges`, exactly the
        // partial import this module's doc comment says never to hand out.
        Some(_) => return Err(bad(format!("formation '{name}' has a 'ranges' that is not a list"))),
        None => {
            let r = get("range")
                .and_then(number)
                .ok_or_else(|| bad(format!("formation '{name}' has no range")))?;
            vec![r; probes.len()]
        }
    };
    Ok(FormationSpec { name, probes, ranges })
}

/// Read shared YAML text. Every entry must be readable: skipping a malformed
/// one silently would hand the user a partial import they did not ask for.
pub fn parse_formations(text: &str) -> Result<Vec<FormationSpec>, ProbeError> {
    let docs = YamlLoader::load_from_str(text)
        .map_err(|e| ProbeError::BadYaml { message: e.to_string() })?;
    let Some(Yaml::Hash(top)) = docs.into_iter().next() else {
        return Err(ProbeError::NotFormations);
    };
    let entries = top
        .into_iter()
        .find(|(k, _)| matches!(k, Yaml::String(s) if s == "formations"))
        .map(|(_, v)| v)
        .ok_or(ProbeError::NotFormations)?;
    let Yaml::Array(items) = entries else { return Err(ProbeError::NotFormations) };
    items.iter().map(read_spec).collect()
}

/// `want`, or `want copy`, `want copy 2`… — the first name `existing` does not
/// already hold. Matches what the editor's Duplicate button produces, so an
/// imported collision and a hand-made one read the same in EVE's menu.
pub fn unique_name(existing: &[String], want: &str) -> String {
    let taken = |c: &str| existing.iter().any(|e| e == c);
    if !taken(want) {
        return want.to_string();
    }
    let first = format!("{want} copy");
    if !taken(&first) {
        return first;
    }
    (2i64..)
        .map(|n| format!("{want} copy {n}"))
        .find(|c| !taken(c))
        .expect("i64 has a free suffix")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus's own "close" coordinates, plus a probe with bits well below
    /// what any rounded display could carry.
    fn close() -> FormationSpec {
        FormationSpec {
            name: "close".into(),
            probes: vec![
                [-1199120384.0, -115136512.0, -415997952.0],
                [-1199120384.7, 16133054464.3, -415997952.9],
            ],
            ranges: vec![74_798_935_350.0, 74_798_935_350.0],
        }
    }

    #[test]
    fn a_round_trip_preserves_every_coordinate_bit_for_bit() {
        // The whole reason the format is metres. A shared formation that comes
        // back displaced is a silent corruption, and 1e-7 of an AU is 15 km.
        let out = parse_formations(&emit_formations(&[close()])).unwrap();
        assert_eq!(out, vec![close()]);
    }

    #[test]
    fn a_uniform_formation_does_not_emit_a_ranges_key() {
        let text = emit_formations(&[close()]);
        assert!(text.contains("range: 74798935350"), "got:\n{text}");
        assert!(!text.contains("ranges:"), "a uniform formation needs no per-probe list:\n{text}");
    }

    #[test]
    fn whole_metres_are_written_without_a_decimal_point() {
        // Every corpus coordinate is integral, and legibility is this format's
        // other job — a trailing `.0` on all twenty-four of them is noise.
        let text = emit_formations(&[close()]);
        assert!(!text.contains("74798935350.0"), "got:\n{text}");
        assert!(text.contains("-1199120384,"), "got:\n{text}");
        // …and a fractional coordinate still keeps every bit it needs.
        assert!(text.contains("-1199120384.7,"), "got:\n{text}");
    }

    #[test]
    fn a_mixed_formation_emits_ranges_and_survives_the_round_trip() {
        // Flattening a mix to one value is exactly what the editor refuses to
        // do (editor spec §2.3); the exchange format must not reintroduce it.
        let mut f = close();
        f.ranges = vec![74_798_935_350.0, 149_597_870_700.0];
        let text = emit_formations(&[f.clone()]);
        assert!(text.contains("ranges: ["), "got:\n{text}");
        assert_eq!(parse_formations(&text).unwrap(), vec![f]);
    }

    #[test]
    fn a_hand_typed_document_parses() {
        // §2.1 promises the format is hand-editable: no comments, no column
        // alignment, no `ranges`, and a range that is a plain integer.
        let text = "\
formations:
  - name: quick
    range: 74798935350
    probes:
      - [1, 2, 3]
      - [-4.5, 0, 6e3]
";
        let out = parse_formations(text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "quick");
        assert_eq!(out[0].probes, vec![[1.0, 2.0, 3.0], [-4.5, 0.0, 6000.0]]);
        assert_eq!(out[0].ranges, vec![74_798_935_350.0, 74_798_935_350.0]);
    }

    #[test]
    fn comments_are_ignored_including_ones_that_look_like_data() {
        let text = "\
# formations: not really
formations:
  - name: c   # range: 999
    range: 100   # 0.000001 AU
    probes:
      - [1, 2, 3]   # 4 km
";
        let out = parse_formations(text).unwrap();
        assert_eq!(out[0].name, "c");
        assert_eq!(out[0].ranges, vec![100.0]);
    }

    #[test]
    fn several_formations_round_trip_in_order() {
        let mut second = close();
        second.name = "on grid".into();
        second.probes = vec![[1.0, 0.0, 0.0]];
        second.ranges = vec![598_391_482_800.0];
        let both = vec![close(), second];
        assert_eq!(parse_formations(&emit_formations(&both)).unwrap(), both);
    }

    #[test]
    fn a_name_with_a_quote_survives() {
        let mut f = close();
        f.name = "bob's 'best'".into();
        assert_eq!(parse_formations(&emit_formations(&[f.clone()])).unwrap(), vec![f]);
    }

    #[test]
    fn junk_is_bad_yaml_and_the_wrong_file_is_not_formations() {
        assert!(matches!(parse_formations("\t- ["), Err(ProbeError::BadYaml { .. })));
        // A real overview pack: valid YAML, valid mapping, wrong contents.
        assert_eq!(parse_formations("presets:\n  - a\n"), Err(ProbeError::NotFormations));
        assert_eq!(parse_formations("just a string\n"), Err(ProbeError::NotFormations));
    }

    #[test]
    fn a_malformed_entry_is_reported_not_skipped() {
        // Skipping would hand the user a partial import they did not ask for.
        let missing_probe = "formations:\n  - name: a\n    range: 1\n    probes:\n      - [1, 2]\n";
        assert!(matches!(parse_formations(missing_probe), Err(ProbeError::BadYaml { .. })));
        let no_range = "formations:\n  - name: a\n    probes:\n      - [1, 2, 3]\n";
        assert!(matches!(parse_formations(no_range), Err(ProbeError::BadYaml { .. })));
        let no_name = "formations:\n  - range: 1\n    probes:\n      - [1, 2, 3]\n";
        assert!(matches!(parse_formations(no_name), Err(ProbeError::BadYaml { .. })));
    }

    #[test]
    fn a_ranges_that_is_not_a_list_is_reported_not_ignored() {
        // A hand edit that left the brackets off `ranges: 200` must not fall
        // back to `range:` and silently discard the 200.
        let text = "formations:\n  - name: a\n    range: 100\n    ranges: 200\n    probes:\n      - [1, 2, 3]\n      - [4, 5, 6]\n";
        assert!(matches!(parse_formations(text), Err(ProbeError::BadYaml { .. })));
    }

    #[test]
    fn unique_name_suffixes_only_on_a_collision() {
        let held = vec!["close".to_string(), "close copy".to_string()];
        assert_eq!(unique_name(&held, "on grid"), "on grid");
        assert_eq!(unique_name(&held, "close"), "close copy 2");
        assert_eq!(unique_name(&[], "close"), "close");
        assert_eq!(unique_name(&["close".to_string()], "close"), "close copy");
    }
}
