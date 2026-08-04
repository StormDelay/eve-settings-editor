# Probe viewer scenes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw user-editable reference geometry ("scenes") in the probe formation 3D viewer, shipping two drifter-wormhole scenes as worked examples.

**Architecture:** A scene is a YAML file in `<app data dir>/scenes/`. A new `scenes.rs` in the Tauri crate reads the directory, normalizes every distance to metres, and hands the result to the frontend through one read-only command. `probes.ts` converts a compass bearing to a world vector — the only geometry, kept there because it depends on the measured `NORTH_AZ_DEG` / `EAST_SIGN` constants. `ProbeViewer.svelte` gains a picker, draws the objects as context, and gains a "Fit scene" camera button.

**Tech Stack:** Rust (Tauri 2, `yaml-rust2`, `serde`), TypeScript, Svelte 5 runes. Tests: `cargo test` for Rust, `node --test` for pure TS, vitest + `@testing-library/svelte` for components.

Design spec: `docs/superpowers/specs/2026-08-04-probe-scenes-design.md`. Section
references below (§4.2, §4.6…) are to that document.

## Global Constraints

- Branch: `probe-scenes`, already created. Do not work on `master`.
- **`scenes.rs` calls no trigonometric function.** It normalizes units only (km → m). Every angle stays raw until `probes.ts` converts it. Reason: `NORTH_AZ_DEG` and `EAST_SIGN` in `probes.ts` are the product's single calibration knob for which way north is, and forking that across two languages means a future patch has to be applied twice.
- Every distance key in the file format names its own unit: `km`, `radius_km`, `xyz_km`, `xyz_m`. There is no file-level `units:` default and no bare `xyz:` (§4.4).
- A file that will not parse is **reported in `problems`, never skipped silently** (§4.6). One bad file must not stop the others loading.
- Nothing a scene draws may hit-test. All of it gets `pointer-events: none`, joining the viewer's existing rule — a scene marker that swallowed a click meant for a probe would deselect it.
- Rust DTO fields cross to TypeScript **as written** — this codebase sets no `rename_all = "camelCase"` anywhere. `radius_m` in Rust is `radius_m` in TypeScript.
- Comments in this codebase explain *why*, not *what*, and record decisions that were tried and rejected. Match that density; the surrounding files are the reference.

## File Structure

**Create:**

| file | responsibility |
|---|---|
| `app/src-tauri/src/scenes.rs` | The whole scene format: parse, validate, unit-normalize, scan the directory, install built-ins. Plus its unit tests. |
| `app/src-tauri/scenes/drifter-kspace.yaml` | Shipped scene, `include_str!`'d into the binary and installed to disk. |
| `app/src-tauri/scenes/drifter-jspace.yaml` | Same, J-space side. |

**Modify:**

| file | change |
|---|---|
| `app/src-tauri/Cargo.toml` | add `yaml-rust2 = "0.10"` |
| `app/src-tauri/src/lib.rs` | `mod scenes;`, the `scene_list` command, register it |
| `app/src/lib/api.ts` | `Scene`, `SceneObject`, `SceneList` types and `api.sceneList()` |
| `app/src/lib/probes.ts` | `ScenePos` type and `scenePos()` — the bearing conversion |
| `app/src/lib/probes.test.ts` | checks for that conversion |
| `app/src/lib/ProbeViewer.svelte` | scene picker, drawing, Fit scene |
| `app/src/lib/ProbeViewer.spec.ts` | component checks for the above |
| `app/src/lib/ProbeFormationsView.svelte` | load the list, pass it down, surface `problems` |

`scenes.rs` lives in the Tauri crate, not in `settings-model`, even though the
existing YAML code (`probe_pack.rs`) is in `settings-model`. `settings-model`
models EVE settings files; a scene is not one. `scenes.rs` is a folder of files
under the app data directory, which is exactly what `presets.rs` is, and
`presets.rs` lives in the Tauri crate.

---

### Task 1: The scene format

**Files:**
- Create: `app/src-tauri/src/scenes.rs`
- Modify: `app/src-tauri/Cargo.toml`
- Test: in-module `#[cfg(test)]` in `app/src-tauri/src/scenes.rs`, matching `probe_pack.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub fn scenes_dir(app_data: &Path) -> PathBuf`
  - `pub fn parse_scene(text: &str) -> Result<Scene, String>`
  - `pub fn list(app_data: &Path) -> SceneList`
  - `pub struct Scene { pub name: String, pub objects: Vec<SceneObject> }`
  - `pub struct SceneObject { pub label: String, pub pos: ScenePos, pub radius_m: f64 }`
  - `pub enum ScenePos { Polar { km: f64, bearing: f64, elevation: f64 }, Xyz { m: [f64; 3] } }`
  - `pub struct SceneList { pub scenes: Vec<Scene>, pub problems: Vec<String> }`

- [ ] **Step 1: Add the YAML dependency**

`app/src-tauri/Cargo.toml`, in `[dependencies]`, after the `blue-marshal` line:

```toml
# Scene files (`scenes.rs`). Already this workspace's YAML reader — it is what
# settings-model's probe_pack.rs parses exchange formats with.
yaml-rust2 = "0.10"
```

- [ ] **Step 2: Write the failing tests, and register the module**

Create `app/src-tauri/src/scenes.rs` containing **only** this test module for
now, and in the same step add `mod scenes;` to `app/src-tauri/src/lib.rs`'s
`mod` block, alphabetically after `mod presets;`. Without the registration the
file is not compiled at all and Step 3 would report nothing.

```rust
mod scenes;
```

The test module:

```rust
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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd app/src-tauri && cargo test scenes`
Expected: FAIL to compile — `parse_scene`, `list`, `Scene`, `ScenePos` and `BUILT_INS` are all undefined.

- [ ] **Step 4: Write the two shipped scene files**

These come **before** the implementation: it `include_str!`s them, so the crate
will not compile until they exist.

Create `app/src-tauri/scenes/drifter-kspace.yaml`:

```yaml
# Drifter wormhole — K-space side.
#
# Positions are relative to the FORMATION CENTRE, which is where your ship sits
# when it launches probes. This scene assumes you have warped to the
# "Unidentified Wormhole" beacon at 0 and are launching from there.
#
# Axes are EVE's: X and Z are the horizontal plane, Y is up. +X is west and +Z
# is north — measured in-game, see NORTH_AZ_DEG in probes.ts.
#
# HOW GOOD ARE THESE NUMBERS? Not very. This file is yours — if you measure
# better ones, edit it.
#
#   87.5 km    From a player-made diagram. Published sources say 89 km, ~80 km,
#              ~100 km and (after the March 2026 patch) 75 km. The in-game
#              overview in the capture this was built from read 87 km.
#   bearing    West. The diagram claims west for drifter sites generally, but
#              only one site was photographed, so "generally" is unverified.
#   +17.45     ABOVE the beacon, not below. THE SIGN IS MEASURED: on the
#              tactical overlay the hole projects 82.8 px from the ship where an
#              object at its own 87 km range lying in the horizontal plane would
#              sit at 53.8 px. Every published source says ~14 degrees BELOW and
#              is wrong about it. The 17.45 itself is the diagram's, and one
#              frame cannot pin it — it comes out at 13 degrees if the camera
#              pitch was 15, 17.5 at 20, and 33 at 30.
#   16 km      The jump sphere. Sourced from a player guide, not measured.
scene:
  name: 'Drifter wormhole — K-space side'
  objects:
    - label: 'Beacon'
      km: 0
    - label: 'Wormhole'
      km: 87.5
      bearing: 270
      elevation: 17.45
      radius_km: 16

    # Coordinates work too, if that is how your source gives them. Give every
    # object EXACTLY ONE of `km`, `xyz_km` or `xyz_m` — never two, never none.
    #
    # - label: 'Something you measured'
    #   xyz_km: [12.5, 3, -8]
    #
    # - label: 'Something measured in metres'
    #   xyz_m: [12500, 3000, -8000]
```

Create `app/src-tauri/scenes/drifter-jspace.yaml` — the same file with the title,
the `scene.name`, the first object's label and the elevation changed, plus an
honesty note that this side is corroborated by nothing:

```yaml
# Drifter wormhole — J-space side, looking back at the hole you came in through.
#
# Positions are relative to the FORMATION CENTRE, which is where your ship sits
# when it launches probes. This scene assumes you are sitting on the J-space
# side warp-in at 0 and launching from there.
#
# Axes are EVE's: X and Z are the horizontal plane, Y is up. +X is west and +Z
# is north — measured in-game, see NORTH_AZ_DEG in probes.ts.
#
# HOW GOOD ARE THESE NUMBERS? Worse than the K-space file's. This file is
# yours — if you measure better ones, edit it.
#
#   87.5 km    From the same player-made diagram, which gives both sides the
#              same distance.
#   bearing    West, per the same diagram. Unverified.
#   +30        ABOVE. NOTHING HERE IS MEASURED — unlike the K-space side, no
#              capture of this geometry was available, so both the angle and its
#              sign are the diagram's word. The sign is at least consistent with
#              the K-space side, where it WAS measured and where every published
#              source had it backwards.
#   16 km      The jump sphere. Sourced from a player guide, not measured.
scene:
  name: 'Drifter wormhole — J-space side'
  objects:
    - label: 'Warp-in'
      km: 0
    - label: 'Wormhole'
      km: 87.5
      bearing: 270
      elevation: 30
      radius_km: 16

    # Coordinates work too, if that is how your source gives them. Give every
    # object EXACTLY ONE of `km`, `xyz_km` or `xyz_m` — never two, never none.
    #
    # - label: 'Something you measured'
    #   xyz_km: [12.5, 3, -8]
    #
    # - label: 'Something measured in metres'
    #   xyz_m: [12500, 3000, -8000]
```

- [ ] **Step 5: Write the implementation**

Put all of this **above** the `#[cfg(test)]` module in `app/src-tauri/src/scenes.rs`:

```rust
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
    Ok(SceneObject { label, pos, radius_m: num("radius_km")?.unwrap_or(0.0) * M_PER_KM })
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd app/src-tauri && cargo test scenes`
Expected: PASS — 10 tests.

- [ ] **Step 7: Commit**

`cargo clippy` is deliberately NOT run here. `mod scenes;` is private and
nothing outside it calls `list` yet, so `-D warnings` would fail on dead code.
Do **not** silence it with `#[allow(dead_code)]` — Task 2 adds the caller, and
clippy runs there.

```bash
git add app/src-tauri/Cargo.toml app/src-tauri/src/scenes.rs app/src-tauri/src/lib.rs app/src-tauri/scenes/
git commit -m "Read scene files, and ship two drifter ones as worked examples"
```

---

### Task 2: The command and its DTOs

**Files:**
- Modify: `app/src-tauri/src/lib.rs`
- Modify: `app/src/lib/api.ts`

**Interfaces:**
- Consumes: `scenes::list`, `scenes::SceneList` from Task 1.
- Produces:
  - Tauri command `scene_list` taking no arguments, returning `SceneList`.
  - `api.sceneList(): Promise<SceneList>` in `api.ts`.
  - TypeScript `Scene`, `SceneObject`, `SceneList` (`ScenePos` arrives in Task 3).

- [ ] **Step 1: Add the command**

`app/src-tauri/src/lib.rs`. Put it immediately after the `add_probe_formations`
command, keeping the probe-adjacent commands together:

```rust
/// Read-only. There is no write path: a scene is a file the user edits, and an
/// in-app editor is a later slice.
#[tauri::command]
fn scene_list(app: tauri::AppHandle) -> scenes::SceneList {
    scenes::list(&app_dir(&app))
}
```

- [ ] **Step 2: Register it**

`app/src-tauri/src/lib.rs`, in the `invoke_handler` list, on the line after
`probe_yaml, probe_parse_yaml, probe_export, probe_import, add_probe_formations,`:

```rust
            scene_list,
```

- [ ] **Step 3: Verify it compiles**

Run: `cd app/src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no warnings, and no dead-code complaint from Task 1 (the command now uses `list`).

- [ ] **Step 4: Add the frontend types and call**

`app/src/lib/api.ts`. Add after the `FormationSpec` type:

```ts
export interface SceneObject {
  label: string;
  pos: ScenePos;
  /** Metres. 0 draws no sphere. */
  radius_m: number;
}

/** Static reference geometry for the probe viewer, read from a file in the
 * app data directory. Read-only: scenes are edited as text, not in the app. */
export interface Scene {
  name: string;
  objects: SceneObject[];
}

/** `problems` holds one message per file that would not read, so a typo in a
 * hand-edited scene is reported rather than silently missing from the picker. */
export interface SceneList {
  scenes: Scene[];
  problems: string[];
}
```

Add the import at the top of the file, beside the existing imports:

```ts
import type { ScenePos } from "./probes";
```

Re-export it so a consumer needs one import, not two:

```ts
export type { ScenePos };
```

And add to the `api` object, after `addProbeFormations`:

```ts
  /** Every scene on disk, installing the shipped ones on first run. */
  sceneList: () => invoke<SceneList>("scene_list"),
```

> `ScenePos` is defined in `probes.ts`, not here, and Task 3 creates it —
> `api.ts` will not typecheck until that task lands. That direction is
> deliberate: the type is what the geometry consumes, and `probes.ts` must stay
> import-free so it remains `node --test`-able.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Hand the scene list to the frontend"
```

---

### Task 3: Bearing to world vector

**Files:**
- Modify: `app/src/lib/probes.ts`
- Test: `app/src/lib/probes.test.ts`

**Interfaces:**
- Consumes: `M_PER_KM`, `NORTH_AZ_DEG`, `EAST_SIGN`, `toCartesian`, `Vec3` — all already in `probes.ts`.
- Produces:
  - `export type ScenePos = { kind: "polar"; km: number; bearing: number; elevation: number } | { kind: "xyz"; m: Vec3 }`
  - `export function scenePos(p: ScenePos): Vec3` — metres, in the formation's own frame.

This is the only geometry in the whole feature, and a sign error in it is
silent: the scene simply draws mirrored. Hence its own task.

- [ ] **Step 1: Write the failing checks**

`app/src/lib/probes.test.ts`. Add `scenePos` and `type ScenePos` to the import
list at the top of the file, then append these checks at the end:

```ts
// --- scene positions --------------------------------------------------------
// A scene is authored in COMPASS bearings, which is not the azimuth convention
// `toSpherical` uses. These pin the mapping between them. Written against
// NORTH_AZ_DEG rather than hard-coded directions, so moving the knob moves the
// test with it instead of breaking it.
{
  const at = (bearing: number, elevation = 0) =>
    scenePos({ kind: "polar", km: 100, bearing, elevation });
  const [n, e, s, w] = cardinals();

  const alongs = (v: Vec3, dir: Vec3) =>
    near(v[0], dir[0] * 100_000, 1) && near(v[1], dir[1] * 100_000, 1) &&
    near(v[2], dir[2] * 100_000, 1);

  check("bearing 0 is north", alongs(at(0), n.v));
  check("bearing 90 is east", alongs(at(90), e.v));
  check("bearing 180 is south", alongs(at(180), s.v));
  check("bearing 270 is west", alongs(at(270), w.v));

  // The drifter scenes' own object: 87.5 km west, 17.45 degrees UP. The sign of
  // y is the measured fact the whole scene rests on (spec §2.2).
  const hole = scenePos({ kind: "polar", km: 87.5, bearing: 270, elevation: 17.45 });
  check("elevation raises the object", hole[1] > 0);
  check("a raised object keeps its slant range", near(Math.hypot(...hole), 87_500, 1e-3));
  check(
    "elevation is measured from the horizontal plane",
    near(hole[1], 87_500 * Math.sin((17.45 * Math.PI) / 180), 1e-3),
  );

  // Kilometres in, metres out. Getting this backwards is a factor of a million
  // and the object lands somewhere nobody will scroll to.
  check("km are scaled to metres", near(Math.hypot(...at(0)), 100_000, 1e-6));

  // The cartesian form is a pass-through: it arrives from Rust already in
  // metres, and touching it here would be a second place units could go wrong.
  const xyz = scenePos({ kind: "xyz", m: [12500, 3000, -8000] });
  check("xyz passes through untouched", xyz[0] === 12500 && xyz[1] === 3000 && xyz[2] === -8000);
}
```

- [ ] **Step 2: Run the checks to verify they fail**

Run: `cd app && node --test "src/lib/probes.test.ts"`
Expected: FAIL — an ESM link error, `scenePos` is not exported from `./probes.ts`.

- [ ] **Step 3: Write the implementation**

`app/src/lib/probes.ts`. Add immediately after the `cardinals()` function, so
the bearing convention sits with the constants it reads:

```ts
/** A scene object's position exactly as `scenes.rs` hands it over: units
 * already normalized to metres, angles untouched. */
export type ScenePos =
  | { kind: "polar"; km: number; bearing: number; elevation: number }
  | { kind: "xyz"; m: Vec3 };

/** A scene position as a world offset in metres from the formation centre.
 *
 * `bearing` is a COMPASS bearing — 0 north, 90 east, 180 south, 270 west —
 * because that is the frame the geometry is known in ("87.5 km, west, 17.45
 * degrees up"). It is NOT `toSpherical`'s azimuth, and this function is the one
 * place the two meet.
 *
 * The conversion lives here and not in `scenes.rs` on purpose: it reads
 * `NORTH_AZ_DEG` and `EAST_SIGN`, which are the product's single calibration
 * knob for which way north is. A second copy in Rust would mean a patch that
 * moved north had to be applied twice, and the two would drift. */
export function scenePos(p: ScenePos): Vec3 {
  if (p.kind === "xyz") return [p.m[0], p.m[1], p.m[2]];
  return toCartesian({
    r: p.km * M_PER_KM,
    az: NORTH_AZ_DEG + EAST_SIGN * p.bearing,
    el: p.elevation,
  });
}
```

> `toCartesian` is declared further down the file. That is fine — function
> declarations hoist, and `probes.ts` already relies on this (`cardinals` uses
> `Vec3`, declared below it).

- [ ] **Step 4: Run the checks to verify they pass**

Run: `cd app && node --test "src/lib/probes.test.ts"`
Expected: PASS, including the eight new `ok -` lines.

- [ ] **Step 5: Verify the frontend typechecks end to end**

Run: `cd app && npm run check`
Expected: no errors — this is also the first point Task 2's `api.ts` import of
`ScenePos` resolves.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/probes.ts app/src/lib/probes.test.ts
git commit -m "Convert a scene's compass bearing to the formation's own frame"
```

---

### Task 4: Draw the scene

**Files:**
- Modify: `app/src/lib/ProbeViewer.svelte`
- Modify: `app/src/lib/ProbeFormationsView.svelte`
- Test: `app/src/lib/ProbeViewer.spec.ts`

**Interfaces:**
- Consumes: `Scene` from `api.ts` (Task 2), `scenePos` from `probes.ts` (Task 3), and the viewer's existing `projectPoint`, `silhouette`, `fitDistance`.
- Produces: a `scenes: Scene[]` prop on `ProbeViewer`, defaulting to `[]`.

The default matters: `ProbeViewer.spec.ts`'s existing `mount()` helper does not
pass it, and every existing test must keep working untouched.

- [ ] **Step 1: Write the failing component tests**

`app/src/lib/ProbeViewer.spec.ts`. Add to the top-level imports if not already
present (`describe`, `expect`, `test` are), then append this block at the end of
the file:

```ts
/** A scene with one object at the origin and one well off it, the second
 * carrying a volume. Distances are in metres — `scenePos` is not involved on
 * the xyz path, so these are the world coordinates directly. */
const SCENE = {
  name: "Drifter wormhole — K-space side",
  objects: [
    { label: "Beacon", pos: { kind: "xyz" as const, m: [0, 0, 0] as [number, number, number] }, radius_m: 0 },
    {
      label: "Wormhole",
      pos: { kind: "xyz" as const, m: [8e9, 2e9, 0] as [number, number, number] },
      radius_m: 1e9,
    },
  ],
};

function mountWithScene() {
  const { container } = render(ProbeViewer, {
    probes: PROBES,
    ranges: RANGES,
    selected: null,
    formationId: 0,
    scenes: [SCENE],
    onselect: noop,
    onmove: noop,
    oncommit: noop,
  });
  return container;
}

const sceneLabels = (c: Element) =>
  [...c.querySelectorAll(".scene-label")].map((e) => e.textContent);

/** Pick by INDEX, not by setting `.value` to a string. The options carry
 * numeric values, and driving the select through `selectedIndex` sidesteps the
 * question of how Svelte stringifies them. */
async function pick(c: Element, index: number) {
  const picker = c.querySelector(".scene-pick") as HTMLSelectElement;
  picker.selectedIndex = index;
  await fireEvent.change(picker);
}

/** A scene marker's x, by its 0-based position in the scene. */
const markX = (c: Element, n: number) =>
  Number(([...c.querySelectorAll(".scene-mark")][n] as SVGCircleElement).getAttribute("cx"));

describe("scenes", () => {
  test("no scene is showing until one is picked", () => {
    const c = mountWithScene();
    expect(sceneLabels(c)).toEqual([]);
    expect((c.querySelector(".scene-pick") as HTMLSelectElement).selectedIndex).toBe(0); // None
  });

  test("picking a scene draws its objects, and None removes them again", async () => {
    const c = mountWithScene();
    expect(c.querySelector(".scene-pick")).toBeTruthy();

    await pick(c, 1); // index 0 is None, so the first scene is 1
    expect(sceneLabels(c)).toEqual(["Beacon", "Wormhole"]);

    await pick(c, 0);
    expect(sceneLabels(c)).toEqual([]);
  });

  test("an object with a radius draws a volume and one without does not", async () => {
    const c = mountWithScene();
    await pick(c, 1);
    // Two objects, one radius. A zero radius must draw NOTHING rather than a
    // zero-radius circle: `silhouette` returns 0 for it, not null, so the guard
    // has to be the viewer's own.
    expect(c.querySelectorAll(".scene-vol").length).toBe(1);
  });

  test("Fit scene frames the scene", async () => {
    const c = mountWithScene();
    await pick(c, 1);
    // The OFF-CENTRE object. The beacon is at the world origin, which is also
    // the camera target, so it projects to the middle of the viewport at every
    // camera distance and would show no change however the camera moved.
    const before = markX(c, 1);

    const fit = [...c.querySelectorAll("button")].find((b) => b.textContent?.includes("Fit scene"));
    expect(fit).toBeTruthy();
    await fireEvent.click(fit!);

    expect(markX(c, 1)).not.toBe(before);
    // Framing is the whole point, so what matters is that both objects are on
    // screen afterwards.
    for (const m of [...c.querySelectorAll(".scene-mark")] as SVGCircleElement[]) {
      for (const attr of ["cx", "cy"]) {
        const v = Number(m.getAttribute(attr));
        expect(v).toBeGreaterThanOrEqual(0);
        expect(v).toBeLessThanOrEqual(SIZE);
      }
    }
  });

  test("no picker at all when there are no scenes on disk", () => {
    // The existing mount() passes no `scenes`, which is the empty case and the
    // reason the prop has to default.
    expect(mount().querySelector(".scene-pick")).toBeNull();
  });
});
```

There is deliberately **no test that the scene is `pointer-events: none`.**
`vitest.config.ts` sets no `css: true`, so Svelte's scoped styles never reach
jsdom and `getComputedStyle(el).pointerEvents` reads `""` for every element in
the file — such a test would pass or fail for reasons unrelated to the rule. The
existing `.range` pointer-events decision is held by a comment for the same
reason; Step 6 adds the scene's classes to that same rule and comment.
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd app && npm run test:ui -- ProbeViewer`
Expected: FAIL — no `.scene-pick` element, `sceneLabels` empty everywhere.

- [ ] **Step 3: Add the prop, state and derived scene to the viewer**

`app/src/lib/ProbeViewer.svelte`. Extend the import from `./probes` with
`scenePos`, add `import type { Scene } from "./api";`, and extend the `$props()`
destructure and its type with `scenes`:

```svelte
  let { probes, ranges, formationId, selected, scenes = [], onselect, onmove, oncommit }: {
    probes: Vec3[];
    ranges: number[];
    /** The formation on show. The re-fit key: a different formation is a
     * different subject and gets framed, a retyped number is not. */
    formationId: number | null;
    selected: number | null;
    /** Reference geometry available to show alongside the probes. Empty when
     * the app data directory holds none, which is what hides the picker. */
    scenes?: Scene[];
    onselect: (i: number | null) => void;
    onmove: (i: number, p: Vec3) => void;
    oncommit: () => void;
  } = $props();
```

Then, immediately after the `compass` derived block (before the `--- gizmo ---`
comment), add:

```svelte
  // --- scene ---------------------------------------------------------------
  // Static reference geometry loaded from a file: a beacon, a wormhole, the
  // volume you can jump it from. Everything here is CONTEXT — it is drawn under
  // the probes, it never hit-tests, and it does not join the probes' depth sort,
  // for the reason the compass does not.
  //
  // The two scales never have to share a frame. A formation is ~0.5 AU across
  // and a drifter site is ~90 km, so at formation zoom the whole scene is
  // sub-pixel and at scene zoom the probes are far outside it — which is why
  // there are two Fit buttons rather than one cleverer one. An earlier attempt
  // at this (formation-editor spec §8) drew both at one scale and rendered the
  // site at 1e-4 px.

  /** Which scene is showing; -1 is none, and is where it starts. Local state:
   * nothing else reads it and nothing persists it. */
  let sceneIndex = $state(-1);
  const scene = $derived(scenes[sceneIndex] ?? null);
  /** The chosen scene's objects in world metres, paired so `fitScene` and the
   * drawing below agree about what is in it. */
  const sceneWorld = $derived(
    (scene?.objects ?? []).map((o) => ({ label: o.label, p: scenePos(o.pos), radius: o.radius_m })),
  );

  const sceneDrawn = $derived(
    sceneWorld
      .map((o) => {
        const s = projectPoint(o.p, basis, SIZE);
        // A zero radius must draw NOTHING. `silhouette` returns 0 for it, not
        // null, and a zero-radius circle is a stray element in the DOM.
        return s === null
          ? null
          : { label: o.label, s, r: o.radius > 0 ? silhouette(s.dist, o.radius, SIZE) : null };
      })
      .filter((o) => o !== null),
  );

  /** Frame the scene rather than the probes. `fitDistance` already takes
   * positions and a matching radius each, which is exactly a scene. */
  function fitScene() {
    cam = {
      ...cam,
      target: [0, 0, 0],
      dist: fitDistance(sceneWorld.map((o) => o.p), sceneWorld.map((o) => o.radius)),
    };
  }
```

- [ ] **Step 4: Draw it**

`app/src/lib/ProbeViewer.svelte`, in the markup. Insert immediately after the
compass `{/if}` and before the `<defs>` block:

```svelte
    <!-- The scene, with the compass: context, painted under everything. Keyed
         by index, not label — two objects may legitimately share a name. -->
    {#each sceneDrawn as o, i (i)}
      {#if o.r !== null}
        <circle cx={o.s.x} cy={o.s.y} r={o.r} class="scene-vol" />
      {/if}
      <circle cx={o.s.x} cy={o.s.y} r="3.5" class="scene-mark" />
      <text x={o.s.x + 7} y={o.s.y - 5} class="scene-label">{o.label}</text>
    {/each}
```

- [ ] **Step 5: Add the picker and the Fit scene button**

`app/src/lib/ProbeViewer.svelte`, in `.viewer-actions`, after the `Fit` button
and before the Vectors `<label>`:

```svelte
    {#if scenes.length}
      <label class="toggle">
        Scene
        <select class="scene-pick" bind:value={sceneIndex}>
          <option value={-1}>None</option>
          {#each scenes as s, i (i)}
            <option value={i}>{s.name}</option>
          {/each}
        </select>
      </label>
      {#if scene}
        <button onclick={fitScene}>Fit scene</button>
      {/if}
    {/if}
```

- [ ] **Step 6: Style it**

`app/src/lib/ProbeViewer.svelte`, in `<style>`. Extend the existing
`pointer-events: none` rule — this is the whole reason a scene marker cannot
steal a probe's click:

```css
  .axis, .axis-label, .range, .vec, .vec-head, .ring, .cardinal,
  .scene-vol, .scene-mark, .scene-label { pointer-events: none; }
```

and add, next to the `.ring` / `.cardinal` rules:

```css
  /* Brighter than the compass, dimmer than a probe. A scene is the thing the
     probes are being placed against, so it has to be findable — but the probes
     are still the subject and the scene must not compete with them. */
  .scene-vol { fill: rgba(255, 255, 255, 0.035); stroke: var(--border); stroke-width: 1; }
  .scene-mark { fill: var(--fg); opacity: 0.75; }
  .scene-label { fill: var(--fg-dim); font-size: 10px; }
```

- [ ] **Step 7: Run the component tests**

Run: `cd app && npm run test:ui -- ProbeViewer`
Expected: PASS, including every pre-existing test in the file.

- [ ] **Step 8: Wire the list into the view**

`app/src/lib/ProbeFormationsView.svelte`. Add `type Scene` to the existing
`./api` import, then add beside the other `$state` declarations near the top:

```svelte
  /** Scenes are read once. They are files on disk, independent of which account
   * or formation is open, so nothing reactive belongs in the effect below. */
  let scenes = $state<Scene[]>([]);
  let sceneProblems = $state<string[]>([]);
  $effect(() => {
    // No reactive reads, so this runs once on mount.
    api
      .sceneList()
      .then((r) => { scenes = r.scenes; sceneProblems = r.problems; })
      // A scene is decoration. Failing to read the directory must not take the
      // formation editor down with it, so this reports nothing and shows none.
      .catch(() => {});
  });
```

Pass them to the viewer — replace the existing `<ProbeViewer …>` invocation's
attribute list by adding one line:

```svelte
        <ProbeViewer probes={draftProbes} ranges={draftRanges} formationId={selectedId}
                     selected={selectedProbe}
                     {scenes}
                     onselect={(i) => (selectedProbe = i)}
                     onmove={moveProbe}
                     oncommit={() => { if (draftChanged()) commit(); }} />
```

And surface the problems immediately after the self-closing `<ProbeViewer … />`
element, still inside the same `<section>`:

```svelte
        {#each sceneProblems as p (p)}
          <p class="hint">Scene not loaded — {p}</p>
        {/each}
```

- [ ] **Step 9: Verify the whole frontend**

Run: `cd app && npm run check && npm test`
Expected: no type errors; every `node --test` check and every vitest spec passes.

- [ ] **Step 10: Commit**

```bash
git add app/src/lib/ProbeViewer.svelte app/src/lib/ProbeViewer.spec.ts app/src/lib/ProbeFormationsView.svelte
git commit -m "Draw a scene in the probe viewer, and frame it on demand"
```

---

### Task 5: Changelog and small-tasks ledger

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `docs/small-tasks.md`

- [ ] **Step 1: Add the changelog entry**

`CHANGELOG.md`. The file's `## [Unreleased]` heading is currently empty; replace
it with exactly this, matching the `### Added` / single-line-bullet shape every
released section uses:

```markdown
## [Unreleased]

### Added
- Reference scenes in the probe formation's 3D view — a beacon, a wormhole and the volume you can jump it from — loaded from editable files, with two drifter-wormhole scenes included.
```

One line per bullet, no wrapping and no engineering detail: that is this
project's release-notes convention and the file is consistent about it.

- [ ] **Step 2: Add the open item the design defers**

`docs/small-tasks.md`, at the top of the **Open** section:

```markdown
- [ ] **Measure the drifter geometry properly, in-client.** The two shipped
  scenes (`app/src-tauri/scenes/`) carry numbers that are estimates, and their
  own comments say so. What IS measured is only the sign of the K-space
  elevation — the hole is above the beacon, not the ~14° below every published
  source claims. The distance (87.5 km against sources saying 75/80/89/91/100),
  the bearing being west for every site rather than the one photographed, the
  J-space 30°, and both 16 km jump spheres are all unverified. Warping to a
  drifter beacon and reading the overlay with a known camera pitch settles all
  of them. _Added 2026-08-04 (probe viewer scenes)._

- [ ] **A distance readout from each probe to a named scene object.** "Is probe
  3 inside the jump sphere" is a number, and the picture only approximates it.
  Deliberately left out of the scenes slice to keep it to drawing; needs no
  change to the scene file format when it arrives. _Added 2026-08-04 (probe
  viewer scenes)._
```

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md docs/small-tasks.md
git commit -m "Note the scenes feature, and what its numbers still owe a client"
```

---

## Verification before the branch is finished

Run all three, from a clean tree:

```bash
cd app/src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
cd ../ && npm run check && npm test
```

Then run the app (`npm run tauri dev`), open an account with probe formations,
and confirm by eye:

1. The Scene picker lists both drifter scenes.
2. Picking one draws a Beacon and a Wormhole label. At the opening zoom they sit
   on top of each other in the middle — that is correct, the site is ~90 km and
   the formation is millions of km.
3. **Fit scene** pulls the camera in until the two separate and the 16 km jump
   sphere is a visible circle. The probes and their range spheres vanish, which
   is also correct.
4. **Fit** puts it back.
5. Editing `<app data>/scenes/drifter-kspace.yaml` — change the elevation to
   `-17.45` — and reopening the view moves the wormhole below the beacon. This
   is the check that the files on disk are really what is being read.
