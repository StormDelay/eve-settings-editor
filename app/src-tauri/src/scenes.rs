//! Scenes: static reference geometry drawn in the probe viewer. A scene names
//! things that sit around your ship — a beacon, a wormhole, the volume you can
//! jump into it from — so a formation can be placed against something real.
//!
//! Every position is relative to the FORMATION CENTRE, which is where the ship
//! sits when it launches probes. A scene is therefore implicitly a sentence of
//! the form "assuming you are sitting here, these things are around you", and
//! each shipped file says where "here" is.
//!
//! KILOMETRES, not metres, and polar as a first-class form. `probe_pack.rs` is
//! metres-exact because a formation round-trips back into a settings file and a
//! rounded value comes back displaced; a scene never returns to EVE, and its
//! geometry is known as "87.5 km, west, 17.45 degrees up". Authoring that in
//! metres would be hostile for no benefit.
//!
//! NO TRIGONOMETRY HERE. This module scales units and nothing else. The bearing
//! convention depends on `NORTH_AZ_DEG` and `EAST_SIGN`, which are the
//! product's single calibration knob for which way north is and live in
//! `probes.ts`; converting a bearing here would fork that knob across two
//! languages, so a future patch that moves north would have to be applied
//! twice.

use std::path::{Path, PathBuf};

use serde::Serialize;
use yaml_rust2::{Yaml, YamlLoader};

const M_PER_KM: f64 = 1000.0;

/// The scene files shipped with the app, embedded so a fresh install has
/// worked examples on disk without a network round trip.
const BUILT_INS: &[(&str, &str)] = &[
    ("drifter-kspace.yaml", include_str!("../scenes/drifter-kspace.yaml")),
    ("drifter-jspace.yaml", include_str!("../scenes/drifter-jspace.yaml")),
];

/// `<app data dir>/scenes` — alongside `presets/` rather than
/// `preferences.json`: scenes are user data, not configuration.
pub fn scenes_dir(app_data: &Path) -> PathBuf {
    app_data.join("scenes")
}

/// A position as the frontend receives it: units normalized to metres, angles
/// exactly as written.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScenePos {
    /// `bearing` is a COMPASS bearing in degrees — 0 north, 90 east — not the
    /// azimuth `toSpherical` uses. `probes.ts` maps between them.
    Polar { km: f64, bearing: f64, elevation: f64 },
    /// Metre offsets on EVE's axes: X and Z horizontal, Y up.
    Xyz { m: [f64; 3] },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneObject {
    pub label: String,
    pub pos: ScenePos,
    /// Metres. 0 draws no sphere.
    pub radius_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scene {
    pub name: String,
    pub objects: Vec<SceneObject>,
}

#[derive(Debug, Default, Serialize)]
pub struct SceneList {
    pub scenes: Vec<Scene>,
    /// One message per file that would not read. Reported rather than skipped:
    /// a typo in a hand-edited scene that made it silently vanish from the
    /// picker is indistinguishable from the app having lost it.
    pub problems: Vec<String>,
}

/// A YAML scalar as a number. `Real` carries the source text; `String` covers a
/// quoted or exponent-formatted value a hand edit might produce, and refusing
/// those would fail a file the user could not see anything wrong with. Same
/// reading `probe_pack.rs` takes.
fn number(y: &Yaml) -> Option<f64> {
    match y {
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::Real(s) | Yaml::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn read_object(y: &Yaml, scene: &str) -> Result<SceneObject, String> {
    let Yaml::Hash(h) = y else {
        return Err(format!("an object of '{scene}' is not a mapping"));
    };
    let get = |k: &str| h.get(&Yaml::String(k.to_string()));
    let label = match get("label") {
        Some(Yaml::String(s)) => s.clone(),
        // `label: 2` is a YAML integer, and a scene that numbers its objects is
        // ordinary. Refusing it would be a puzzle, not a diagnosis.
        Some(Yaml::Integer(i)) => i.to_string(),
        _ => return Err(format!("an object of '{scene}' has no label")),
    };
    let num = |k: &str| -> Result<Option<f64>, String> {
        match get(k) {
            None => Ok(None),
            Some(v) => number(v)
                .map(Some)
                .ok_or_else(|| format!("'{k}' of '{label}' in '{scene}' is not a number")),
        }
    };
    let triple = |k: &str| -> Result<Option<[f64; 3]>, String> {
        match get(k) {
            None => Ok(None),
            Some(Yaml::Array(a)) if a.len() == 3 => {
                let mut out = [0.0f64; 3];
                for (slot, cell) in out.iter_mut().zip(a) {
                    *slot = number(cell).ok_or_else(|| {
                        format!("a coordinate of '{label}' in '{scene}' is not a number")
                    })?;
                }
                Ok(Some(out))
            }
            Some(_) => {
                Err(format!("'{k}' of '{label}' in '{scene}' is not a list of three numbers"))
            }
        }
    };

    let km = num("km")?;
    let xyz_km = triple("xyz_km")?;
    let xyz_m = triple("xyz_m")?;
    // Exactly one form. Neither is not a default to the origin: an object that
    // quietly landed there would be indistinguishable from the beacon, which is
    // the one place a mistake is hardest to spot.
    match [km.is_some(), xyz_km.is_some(), xyz_m.is_some()].iter().filter(|f| **f).count() {
        1 => {}
        0 => {
            return Err(format!(
                "'{label}' in '{scene}' has no position: give it 'km', 'xyz_km' or 'xyz_m'"
            ))
        }
        _ => {
            return Err(format!(
                "'{label}' in '{scene}' has more than one position: give it only one of 'km', 'xyz_km' or 'xyz_m'"
            ))
        }
    }

    let pos = if let Some(km) = km {
        ScenePos::Polar {
            km,
            bearing: num("bearing")?.unwrap_or(0.0),
            elevation: num("elevation")?.unwrap_or(0.0),
        }
    } else if let Some(v) = xyz_km {
        ScenePos::Xyz { m: [v[0] * M_PER_KM, v[1] * M_PER_KM, v[2] * M_PER_KM] }
    } else {
        ScenePos::Xyz { m: xyz_m.expect("exactly one form is present") }
    };
    // Computed before the struct literal, not inline: `num` still borrows
    // `label`, and the field order in a literal would otherwise move `label`
    // ahead of this call.
    let radius_m = num("radius_km")?.unwrap_or(0.0) * M_PER_KM;
    Ok(SceneObject { label, pos, radius_m })
}

/// Read one scene file.
pub fn parse_scene(text: &str) -> Result<Scene, String> {
    // `yaml-rust2` strips a BOM only on its byte-reading path, which this does
    // not use, and neither does `fs::read_to_string`. Left in, U+FEFF is
    // scanned into the first key, `scene` misses, and a valid file is reported
    // as the wrong one. Several editors and PowerShell write one.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let docs = YamlLoader::load_from_str(text).map_err(|e| e.to_string())?;
    if docs.len() > 1 {
        return Err("this holds several YAML documents; keep the scene in one, with no '---' separator".into());
    }
    let not_a_scene = || "this is not a scene file: it has no 'scene' section".to_string();
    let Some(Yaml::Hash(top)) = docs.into_iter().next() else { return Err(not_a_scene()) };
    let Some(Yaml::Hash(scene)) = top.get(&Yaml::String("scene".into())).cloned() else {
        return Err(not_a_scene());
    };
    let get = |k: &str| scene.get(&Yaml::String(k.to_string()));
    let name = match get("name") {
        Some(Yaml::String(s)) => s.clone(),
        Some(Yaml::Integer(i)) => i.to_string(),
        _ => return Err("the scene has no name".into()),
    };
    let Some(Yaml::Array(rows)) = get("objects") else {
        return Err(format!("scene '{name}' has no objects list"));
    };
    let objects =
        rows.iter().map(|r| read_object(r, &name)).collect::<Result<Vec<_>, String>>()?;
    Ok(Scene { name, objects })
}

// ponytail: `list` re-reads and re-parses every file on every call. Scene files
// are a handful of small documents and the list is only rebuilt when the view
// mounts. Cache by (path, mtime) if a large library ever drags — the same call
// `presets::list` makes, for the same reason.
/// Every scene on disk, sorted by name, plus a message for each file that would
/// not read.
///
/// Installs the built-ins the first time the directory is ABSENT. The guard is
/// on the directory and not on each file, so a scene the user deleted stays
/// deleted and one they edited is never overwritten — which is the point of
/// installing files rather than reading them out of the binary.
pub fn list(app_data: &Path) -> SceneList {
    let root = scenes_dir(app_data);
    let mut out = SceneList::default();
    if !root.exists() && std::fs::create_dir_all(&root).is_ok() {
        for (name, body) in BUILT_INS {
            // Best effort. An unwritable app data directory is not a reason to
            // show no scenes at all; the built-ins are simply absent this run.
            let _ = std::fs::write(root.join(name), body);
        }
    }
    let Ok(entries) = std::fs::read_dir(&root) else { return out };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            // `.yml` too: it costs one condition, and a file ignored for its
            // extension is the same silent disappearance `problems` exists to
            // prevent.
            p.is_file()
                && p.extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml")
                })
        })
        .collect();
    files.sort();
    for path in files {
        let shown = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let read = std::fs::read_to_string(&path).map_err(|e| e.to_string());
        match read.and_then(|t| parse_scene(&t)) {
            Ok(s) => out.scenes.push(s),
            Err(e) => out.problems.push(format!("{shown}: {e}")),
        }
    }
    // Stable, so two files claiming one name keep their filename order rather
    // than swapping about between runs. They are NOT deduplicated: the file is
    // the identity, and hiding one would make a copied-and-edited scene look
    // like it had failed to load.
    out.scenes.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLAR: &str = "\
scene:
  name: 'Drifter'
  objects:
    - label: 'Beacon'
      km: 0
    - label: 'Wormhole'
      km: 87.5
      bearing: 270
      elevation: 17.45
      radius_km: 16
";

    #[test]
    fn a_polar_object_keeps_its_angles_and_normalizes_its_distances() {
        // The angles are NOT converted here: `probes.ts` owns the bearing
        // convention, because it owns NORTH_AZ_DEG. This module only scales
        // units.
        let s = parse_scene(POLAR).unwrap();
        assert_eq!(s.name, "Drifter");
        assert_eq!(s.objects.len(), 2);
        assert_eq!(s.objects[0].pos, ScenePos::Polar { km: 0.0, bearing: 0.0, elevation: 0.0 });
        assert_eq!(s.objects[0].radius_m, 0.0);
        assert_eq!(
            s.objects[1].pos,
            ScenePos::Polar { km: 87.5, bearing: 270.0, elevation: 17.45 },
        );
        assert_eq!(s.objects[1].radius_m, 16_000.0);
    }

    #[test]
    fn both_cartesian_forms_describe_the_same_point() {
        // The whole reason `xyz_m` exists: EVE's own coordinates are metres, so
        // a source that has them should not need a division. The two spellings
        // must not disagree.
        let km = parse_scene(
            "scene:\n  name: a\n  objects:\n    - label: p\n      xyz_km: [12.5, 3, -8]\n",
        )
        .unwrap();
        let m = parse_scene(
            "scene:\n  name: a\n  objects:\n    - label: p\n      xyz_m: [12500, 3000, -8000]\n",
        )
        .unwrap();
        assert_eq!(km.objects[0].pos, ScenePos::Xyz { m: [12500.0, 3000.0, -8000.0] });
        assert_eq!(km.objects[0].pos, m.objects[0].pos);
    }

    #[test]
    fn an_object_needs_exactly_one_position_form() {
        // Neither: an object that quietly landed at the origin would be
        // indistinguishable from the beacon, which is the one place a mistake
        // is hardest to spot (spec §4.6).
        let none = "scene:\n  name: a\n  objects:\n    - label: p\n";
        let e = parse_scene(none).unwrap_err();
        assert!(e.contains("'p'") && e.contains("no position"), "got: {e}");

        let both = "scene:\n  name: a\n  objects:\n    - label: p\n      km: 5\n      xyz_m: [1, 2, 3]\n";
        let e = parse_scene(both).unwrap_err();
        assert!(e.contains("'p'") && e.contains("more than one position"), "got: {e}");
    }

    #[test]
    fn a_malformed_number_or_triple_is_named_not_defaulted() {
        let bad_num = "scene:\n  name: a\n  objects:\n    - label: p\n      km: west\n";
        assert!(parse_scene(bad_num).unwrap_err().contains("'km'"));
        let short = "scene:\n  name: a\n  objects:\n    - label: p\n      xyz_km: [1, 2]\n";
        assert!(parse_scene(short).unwrap_err().contains("xyz_km"));
    }

    #[test]
    fn the_wrong_file_is_not_a_scene() {
        // A real probe formation pack: valid YAML, valid mapping, wrong file.
        assert!(parse_scene("formations:\n  - name: a\n").unwrap_err().contains("not a scene"));
        assert!(parse_scene("just a string\n").unwrap_err().contains("not a scene"));
    }

    #[test]
    fn a_byte_order_mark_does_not_make_a_valid_file_the_wrong_file() {
        // PowerShell's `Out-File -Encoding utf8` and several editors write one,
        // and it lands INSIDE the first key, so `scene` misses and the user is
        // told they picked the wrong file. Same trap probe_pack.rs hit.
        let s = parse_scene(&format!("\u{feff}{POLAR}")).unwrap();
        assert_eq!(s.name, "Drifter");
    }

    #[test]
    fn listing_installs_the_built_ins_once_and_never_overwrites_them() {
        let dir = tempdir();
        let first = list(&dir);
        assert_eq!(first.problems, Vec::<String>::new());
        assert_eq!(first.scenes.len(), BUILT_INS.len(), "the built-ins should be installed");

        // An edit survives. This is the point of installing files rather than
        // reading them out of the binary: they are the user's to change.
        let edited = scenes_dir(&dir).join(BUILT_INS[0].0);
        std::fs::write(&edited, "scene:\n  name: mine\n  objects:\n    - label: p\n      km: 1\n")
            .unwrap();
        let second = list(&dir);
        assert!(second.scenes.iter().any(|s| s.name == "mine"), "an edit must not be overwritten");

        // A deleted scene stays deleted: the install guard is on the DIRECTORY.
        std::fs::remove_file(&edited).unwrap();
        assert_eq!(list(&dir).scenes.len(), BUILT_INS.len() - 1);
    }

    #[test]
    fn one_unreadable_file_is_reported_while_the_others_still_load() {
        // The difference from `parse_formations`, which refuses wholesale: a
        // directory scan is many independent files, and refusing all of them
        // because one has a typo helps nobody. But it must be REPORTED — a
        // typo that made a scene silently vanish from the picker is the
        // failure this whole field exists to prevent (spec §4.6).
        let dir = tempdir();
        list(&dir); // install
        std::fs::write(scenes_dir(&dir).join("broken.yaml"), "\t- [").unwrap();
        let out = list(&dir);
        assert_eq!(out.scenes.len(), BUILT_INS.len(), "the good files still load");
        assert_eq!(out.problems.len(), 1);
        assert!(out.problems[0].starts_with("broken.yaml:"), "got: {:?}", out.problems);
    }

    #[test]
    fn scenes_come_back_sorted_by_name() {
        let dir = tempdir();
        std::fs::create_dir_all(scenes_dir(&dir)).unwrap();
        for n in ["zulu", "alpha"] {
            std::fs::write(
                scenes_dir(&dir).join(format!("{n}.yaml")),
                format!("scene:\n  name: {n}\n  objects:\n    - label: p\n      km: 1\n"),
            )
            .unwrap();
        }
        let names: Vec<String> = list(&dir).scenes.into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["alpha".to_string(), "zulu".to_string()]);
    }

    #[test]
    fn every_shipped_scene_parses() {
        // The built-ins are hand-written text embedded in the binary. Nothing
        // else would catch a typo in one before a user saw an empty picker.
        for (name, body) in BUILT_INS {
            parse_scene(body).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    /// A fresh empty directory that outlives the test body.
    ///
    /// No `tempfile` dependency: this crate has none, and the only thing
    /// needed is a unique path. The nanosecond clock plus the test's own
    /// thread name is unique enough for a test binary.
    fn tempdir() -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("ese-scenes-{stamp}-{:?}", std::thread::current().id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
