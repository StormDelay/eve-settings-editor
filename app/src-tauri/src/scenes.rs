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

/// The scenes shipped with the app. Rewritten from the binary on every launch,
/// so an improved measurement reaches an existing install instead of only a
/// fresh one. NOT the user's to edit — `scenes/` itself is.
pub fn shipped_dir(app_data: &Path) -> PathBuf {
    scenes_dir(app_data).join("shipped")
}

/// The YAML files directly inside `dir`, sorted. Not recursive: `scenes/` holds
/// `shipped/`, and this must not walk into it and list its contents twice.
fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
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
    files
}

/// Whether `shipped` really resolves to a directory inside `root`, rather than
/// to somewhere else entirely.
///
/// The ownership split rests on exactly one invariant — the app only ever writes
/// inside `shipped/` — and a reparse point at that path breaks it silently.
/// `create_dir_all` FOLLOWS a symlink or NTFS junction and reports success, and
/// `fs::write` would then write straight through it. The realistic harm is not
/// privilege escalation, since the app writes as the user either way: it is a
/// junction aimed back at `scenes/`, which would turn every launch into an
/// overwrite of the user's own scenes.
///
/// Canonicalizing both and comparing settles the whole class in one comparison
/// rather than enumerating reparse tags — `is_symlink` alone would miss a
/// junction, which carries a different tag from a symlink on Windows.
///
/// The check is deliberately looser than the invariant: it accepts `shipped`
/// resolving to any descendant of `root`, not specifically `root/shipped`. A
/// junction aimed at a sibling under `scenes/` therefore still gets written. That
/// costs nothing anyone can see — `list` never scans nested directories, so such
/// a directory is not in the picker either way — and tightening it would trade a
/// real comparison for a stricter-looking one.
///
/// A check-then-write is a race in principle: `fs::write` re-resolves the path,
/// so a reparse point installed between this call and the writes would still be
/// followed. Closing that needs directory-handle-relative I/O, which `std::fs`
/// does not expose portably. Before this check any pre-existing junction worked
/// forever and needed no timing at all; now it needs a writer racing the exact
/// moment `list` runs, which is a different kind of problem.
fn writes_stay_inside(root: &Path, shipped: &Path) -> bool {
    match (root.canonicalize(), shipped.canonicalize()) {
        // `s != r` because a junction can point at its own parent, and that is
        // the case that costs the user their files rather than merely misfiling
        // ours.
        (Ok(r), Ok(s)) => s.starts_with(&r) && s != r,
        // Unresolvable: refuse rather than guess. The shipped scenes go missing
        // for this run, the same degradation as an unwritable directory.
        _ => false,
    }
}

// ponytail: `list` re-reads and re-parses every file on every call. Scene files
// are a handful of small documents and the list is only rebuilt when the view
// mounts. Cache by (path, mtime) if a large library ever drags — the same call
// `presets::list` makes, for the same reason.
/// Every scene on disk, sorted by name, plus a message for each file that would
/// not read.
///
/// TWO DIRECTORIES, AND THE SPLIT IS OWNERSHIP. `scenes/shipped/` belongs to the
/// app and is overwritten wholesale every time this runs; `scenes/` itself
/// belongs to the user and is never written to. To change a shipped scene you
/// copy it up one level and edit the copy, which is the `/usr/share` versus
/// `~/.config` arrangement and is legible from the path alone.
///
/// An earlier design wrote the built-ins straight into `scenes/` and guarded on
/// the directory being absent, so that a deleted scene stayed deleted. That was
/// the wrong trade: it also meant a release that improved the drifter numbers —
/// which the numbers themselves invite, being estimates — could never reach
/// anyone who had already run the app once. Refreshing beats remembering a
/// deletion, and this way neither promise has to be broken, because the two
/// kinds of file no longer share a directory.
pub fn list(app_data: &Path) -> SceneList {
    let root = scenes_dir(app_data);
    let shipped = shipped_dir(app_data);
    let mut out = SceneList::default();
    // Best effort throughout. An unwritable app data directory is not a reason
    // to show no scenes at all; the shipped ones are simply absent this run.
    if std::fs::create_dir_all(&shipped).is_ok() && writes_stay_inside(&root, &shipped) {
        for (name, body) in BUILT_INS {
            let _ = std::fs::write(shipped.join(name), body);
        }
    }
    // The user's own first, then the shipped ones. Only the ordering of the
    // reads — `scenes` is sorted by name below regardless.
    let files = yaml_files(&root).into_iter().chain(yaml_files(&shipped));
    for path in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        // A shipped file says so, or two files of the same name in the two
        // directories produce the same message and neither can be found.
        let shown =
            if path.starts_with(&shipped) { format!("shipped/{name}") } else { name };
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
    fn a_shipped_scene_is_restored_however_it_was_changed() {
        // The whole reason `shipped/` is a separate directory: a release that
        // improves the drifter numbers has to reach an install that has already
        // run, not just a fresh one. Editing or deleting a shipped file is
        // therefore undone, and that is not data loss, because the file was
        // never the user's — `scenes/` is (see the test below).
        let dir = tempdir();
        let first = list(&dir);
        assert_eq!(first.problems, Vec::<String>::new());
        assert_eq!(first.scenes.len(), BUILT_INS.len(), "the shipped scenes should be installed");

        let shipped = shipped_dir(&dir).join(BUILT_INS[0].0);
        let original = std::fs::read_to_string(&shipped).unwrap();
        std::fs::write(&shipped, "scene:\n  name: mine\n  objects:\n    - label: p\n      km: 1\n")
            .unwrap();
        let after_edit = list(&dir);
        assert!(
            !after_edit.scenes.iter().any(|s| s.name == "mine"),
            "an edit to a shipped scene must be overwritten",
        );
        assert_eq!(std::fs::read_to_string(&shipped).unwrap(), original);

        std::fs::remove_file(&shipped).unwrap();
        assert_eq!(list(&dir).scenes.len(), BUILT_INS.len(), "a deleted shipped scene comes back");
    }

    #[test]
    fn a_users_own_scene_is_never_written_to() {
        // The other half of the split. `scenes/` is the user's: what they put
        // there is listed beside the shipped ones and is never touched, which
        // is what makes "copy it up a level and edit the copy" a real answer
        // rather than advice that quietly fails.
        let dir = tempdir();
        list(&dir); // install
        let mine = scenes_dir(&dir).join(BUILT_INS[0].0); // deliberately the SAME name
        let body = "scene:\n  name: mine\n  objects:\n    - label: p\n      km: 1\n";
        std::fs::write(&mine, body).unwrap();

        let out = list(&dir);
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), body, "the user's file is untouched");
        // Both are listed. A name collision is not deduplicated: the file is the
        // identity, and hiding one would make a copied-and-edited scene look
        // like it had failed to load.
        assert_eq!(out.scenes.len(), BUILT_INS.len() + 1);
        assert!(out.scenes.iter().any(|s| s.name == "mine"));
    }

    #[test]
    fn a_file_where_the_shipped_directory_belongs_costs_only_the_shipped_scenes() {
        // `create_dir_all` fails, the write loop is skipped, and the user's own
        // scenes still come back. The "best effort" the module claims, pinned.
        let dir = tempdir();
        let root = scenes_dir(&dir);
        std::fs::create_dir_all(&root).unwrap();
        let body = "scene:\n  name: mine\n  objects:\n    - label: p\n      km: 1\n";
        std::fs::write(root.join("mine.yaml"), body).unwrap();
        std::fs::write(shipped_dir(&dir), b"not a directory").unwrap();

        let out = list(&dir);
        assert_eq!(out.scenes.len(), 1, "the shipped scenes are simply absent");
        assert_eq!(out.scenes[0].name, "mine");
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_where_the_shipped_directory_belongs_is_not_written_through() {
        // THE case `writes_stay_inside` exists for, in its worst form: a
        // junction at `scenes/shipped` aimed back at `scenes/`, holding a file
        // with the same name as a shipped scene. Unguarded, every launch
        // overwrites it.
        //
        // Reachable by any standard user — `mklink /J` needs neither elevation
        // nor Developer Mode, unlike a true symlink. That asymmetry is the
        // whole reason the guard canonicalizes instead of asking `is_symlink`,
        // and it is why this test can exist at all.
        let dir = tempdir();
        let root = scenes_dir(&dir);
        std::fs::create_dir_all(&root).unwrap();
        let mine = root.join(BUILT_INS[0].0); // deliberately a shipped name
        let body = "scene:\n  name: mine\n  objects:\n    - label: p\n      km: 1\n";
        std::fs::write(&mine, body).unwrap();

        // `raw_arg`, not `args`. `Command` quotes any argument holding spaces or
        // quotes, which turns the command line into `/C "mklink /J \"a\" \"b\""`
        // — cmd does not read backslash-escaped quotes and reports a syntax
        // error. `raw_arg` appends the line verbatim, which is the only way to
        // hand cmd a quoted path from Rust.
        use std::os::windows::process::CommandExt;
        let line = format!(
            "/C mklink /J \"{}\" \"{}\"",
            shipped_dir(&dir).display(),
            root.display(),
        );
        let out = std::process::Command::new("cmd")
            .raw_arg(&line)
            .output()
            .expect("cmd is available on Windows");
        assert!(
            out.status.success(),
            "mklink failed ({line}): {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );

        list(&dir);
        assert_eq!(
            std::fs::read_to_string(&mine).unwrap(),
            body,
            "the user's file must survive a junction pointing at its own directory",
        );
    }

    #[test]
    fn a_broken_shipped_file_is_named_as_shipped() {
        // Two files of the same name in the two directories would otherwise
        // produce the same message, and the user could not tell which to fix.
        let dir = tempdir();
        list(&dir);
        std::fs::write(shipped_dir(&dir).join("broken.yaml"), "\t- [").unwrap();
        let out = list(&dir);
        assert_eq!(out.problems.len(), 1);
        assert!(
            out.problems[0].starts_with("shipped/broken.yaml:"),
            "got: {:?}",
            out.problems,
        );
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
        // By NAME, not by which directory the file came from — the shipped
        // scenes are read after the user's but must not clump at the end.
        let names: Vec<String> = list(&dir).scenes.into_iter().map(|s| s.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "got: {names:?}");
        let at = |n: &str| names.iter().position(|s| s == n).unwrap_or_else(|| panic!("no {n}"));
        assert!(at("alpha") < at("zulu"));
        assert_eq!(names.len(), BUILT_INS.len() + 2);
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
    /// needed is a unique path. The nanosecond clock alone is not enough —
    /// tests run in parallel and the clock's resolution is coarser than the
    /// gap between two of them — so a counter goes on the end.
    ///
    /// PLAIN CHARACTERS ONLY. This was `{:?}` of the thread id, which renders
    /// as `ThreadId(3)`, and the junction test hands its path to
    /// `cmd /C mklink`, where parentheses are syntax even inside quotes.
    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("ese-scenes-{stamp}-{n}"));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
