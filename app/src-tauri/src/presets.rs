//! The preset library: a folder of presets, each holding a char-side and an
//! account-side settings document. Nothing here invents a file format — a
//! preset's two files are ordinary settings documents, which is exactly what
//! lets the editors open one as if it were a character.

use std::path::{Component, Path, PathBuf};

use blue_marshal::{encode, Value};
use serde::Serialize;
use settings_model::{apply_to_tree, extract_categories, Category};

use crate::ops::{aspect_writes, Aspect};

pub const CHAR_FILE: &str = "char.dat";
pub const USER_FILE: &str = "user.dat";
/// Written only for a full (Everything) preset. Its absence means "pruned",
/// which is the safe reading: fewer aspects offered, never a destructive full
/// copy built on a partial document.
pub const MARKER_FILE: &str = "preset.json";
/// Claimed inside every preset folder by the save chain's backup step.
const BACKUP_DIR: &str = "eve-settings-editor-backups";

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A rejected preset name, carrying the message shown to the user.
#[derive(Debug)]
pub struct NameError(pub String);

/// `<app data dir>/presets` — alongside accounts.json rather than
/// preferences.json: presets are user data, not configuration.
pub fn presets_dir(app_data: &Path) -> PathBuf {
    app_data.join("presets")
}

/// Validate a user-typed preset name. Rejects rather than silently rewriting,
/// so the name the user sees is the name on disk.
pub fn sanitize_name(raw: &str) -> Result<String, NameError> {
    let bad = |m: &str| Err(NameError(m.to_string()));
    if raw.chars().all(char::is_whitespace) {
        return bad("A preset needs a name.");
    }
    if raw.chars().count() > 100 {
        return bad("A preset name can be at most 100 characters.");
    }
    if let Some(c) = raw.chars().find(|c| "/\\:*?\"<>|".contains(*c) || c.is_control()) {
        return Err(NameError(format!("A preset name cannot contain {c:?}.")));
    }
    if raw.starts_with('.') || raw.ends_with('.') || raw.starts_with(' ') || raw.ends_with(' ') {
        return bad("A preset name cannot start or end with a dot or a space.");
    }
    let stem = raw.split('.').next().unwrap_or(raw);
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return bad("That name is reserved by Windows. Pick another.");
    }
    if raw.eq_ignore_ascii_case(BACKUP_DIR) {
        return bad("That name is used by the editor's own backups.");
    }
    Ok(raw.to_string())
}

/// True when `name` is exactly one ordinary path component — no separators, no
/// `.`/`..`, no drive prefix or root. Checked independently of `sanitize_name`
/// so that a gap in the name rules still cannot escape the presets directory.
fn is_single_normal_component(name: &str) -> bool {
    let mut comps = Path::new(name).components();
    matches!(comps.next(), Some(Component::Normal(_))) && comps.next().is_none()
}

/// The folder a preset lives in. Two independent guards: the name rules above,
/// and a containment check that the name is exactly one ordinary path
/// component — so a gap in the first cannot escape the presets directory.
pub fn preset_path(app_data: &Path, name: &str) -> Result<PathBuf, NameError> {
    let name = sanitize_name(name)?;
    if !is_single_normal_component(&name) {
        return Err(NameError("Invalid preset name.".into()));
    }
    Ok(presets_dir(app_data).join(name))
}

/// The documents a preset is cut from — whichever slots are open.
pub struct CreateInput<'a> {
    pub char_doc: Option<&'a Value>,
    pub user_doc: Option<&'a Value>,
}

/// Whether a document holds anything under `cat`.
pub fn has_category(doc: &Value, cat: Category) -> bool {
    !extract_categories(doc, &[cat]).is_empty()
}

/// The intermediate parent dicts these categories need, so `apply_to_tree`'s
/// insert branch has somewhere to put `ui -> editHistory` and
/// `cmd -> customCmds`. Read out of `Category::key_path` itself, so a category
/// added later needs no change here.
fn parent_entries(cats: &[Category]) -> Vec<(Value, Value)> {
    let mut out: Vec<(Value, Value)> = Vec::new();
    for cat in cats {
        let keys = cat.key_path();
        // ponytail: handles one parent level, which covers every Category today
        // (max depth 2). A three-level key path would silently produce a preset
        // missing that category — make this build the full parent chain if one
        // is ever added.
        debug_assert!(keys.len() <= 2, "prune handles at most one parent level");
        if keys.len() < 2 {
            continue;
        }
        let key = Value::Bytes(keys[0].to_vec());
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, Value::Dict(Vec::new())));
        }
    }
    out
}

/// A document holding only `cats` from `source`, and nothing else.
///
/// Parents are built only for the categories the source ACTUALLY has, so no
/// empty `ui` or `cmd` dict can survive — which also means nothing has to be
/// removed after `apply_to_tree` has already resharded the tree.
pub fn prune(source: &Value, cats: &[Category]) -> Value {
    let extracted = extract_categories(source, cats);
    let present: Vec<Category> = extracted.iter().map(|(c, _)| *c).collect();
    let mut out = Value::Dict(parent_entries(&present));
    apply_to_tree(&mut out, &extracted);
    out
}

/// Write a new preset. The documents come from the open slots (in memory, so
/// unsaved edits are captured — the same choice `pack_export` made).
pub fn create(
    app_data: &Path,
    name: &str,
    aspects: &[Aspect],
    docs: CreateInput<'_>,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let dir = preset_path(app_data, name).map_err(|e| e.0)?;
    if dir.exists() && !overwrite {
        return Err(format!("A preset called \u{201c}{name}\u{201d} already exists."));
    }
    let w = aspect_writes(aspects);
    // Refuse an aspect whose side is not open, rather than writing an empty
    // document that claims to hold it.
    if w.writes_char() && docs.char_doc.is_none() {
        return Err("That needs a character file open.".into());
    }
    if w.writes_account() && docs.user_doc.is_none() {
        return Err("That needs the account file open \u{2014} pair the character first.".into());
    }

    let full = w.char_full_copy || w.account_full_copy;
    let side = |doc: Option<&Value>, cats: &[Category]| match (full, doc) {
        (true, Some(d)) => d.clone(),
        (false, Some(d)) => prune(d, cats),
        (_, None) => Value::Dict(Vec::new()),
    };
    // Encode before touching the filesystem, so an encode failure writes
    // nothing at all. The writes themselves are in place and not atomic: a
    // failure part way through leaves a preset with one side updated, which is
    // recoverable by saving again. It never leaves the user with NO preset,
    // which deleting the folder first would have risked.
    let char_bytes = encode(&side(docs.char_doc, &w.char_categories))
        .map_err(|e| format!("encoding the character side failed: {e}"))?;
    let user_bytes = encode(&side(docs.user_doc, &w.account_categories))
        .map_err(|e| format!("encoding the account side failed: {e}"))?;

    std::fs::create_dir_all(&dir).map_err(|e| format!("creating the preset failed: {e}"))?;
    std::fs::write(dir.join(CHAR_FILE), &char_bytes).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(USER_FILE), &user_bytes).map_err(|e| e.to_string())?;
    if full {
        std::fs::write(dir.join(MARKER_FILE), br#"{"full":true}"#).map_err(|e| e.to_string())?;
    } else {
        // Overwriting a full preset with a pruned one must drop the marker, or
        // the preset would keep claiming to be a complete copy. A marker that
        // was never there is the normal case and not an error; one that refuses
        // to go IS, and must fail the save rather than leave a pruned preset
        // labelled full — that label is what lets a whole-file copy overwrite a
        // target's settings with these three keys.
        match std::fs::remove_file(dir.join(MARKER_FILE)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("clearing the full-preset marker failed: {e}")),
        }
    }
    Ok(dir)
}

/// One row of the library. Everything except `full` is derived by looking at
/// the two documents — there is no stored aspect list to keep in sync.
#[derive(Debug, Clone, Serialize)]
pub struct PresetInfo {
    pub name: String,
    pub dir: String,
    pub char_path: String,
    pub user_path: String,
    pub modified_unix: Option<u64>,
    pub aspects: Vec<Aspect>,
    pub full: bool,
    /// Set when a document failed to decode. Such a preset is still listed —
    /// one that silently vanishes is worse than one that says it is broken.
    pub error: Option<String>,
}

/// Whether the marker file says this preset is a complete copy. Any failure to
/// read or parse it reads as `false`: the safe direction.
pub fn is_full(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(MARKER_FILE))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("full").and_then(|f| f.as_bool()))
        .unwrap_or(false)
}

fn modified_of(paths: [&Path; 2]) -> Option<u64> {
    paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .filter_map(|m| m.modified().ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .max()
}

// ponytail: list decodes every preset on every call. Presets are small (pruned)
// or settings-file sized (full), there will be a handful, and the list is only
// rebuilt on user action. If a large library ever drags, cache by (path, mtime).
/// Every preset in the library, sorted by name. A missing presets directory is
/// an empty library, not an error — and listing never creates it.
pub fn list(app_data: &Path) -> Vec<PresetInfo> {
    let root = presets_dir(app_data);
    let Ok(entries) = std::fs::read_dir(&root) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let (char_path, user_path) = (dir.join(CHAR_FILE), dir.join(USER_FILE));
        // A folder without both documents is not a preset, which is what keeps
        // a stray directory out of the library. (The save chain's backups
        // folder never reaches here at all: it is created INSIDE an individual
        // preset's folder, and this scan does not recurse.)
        if !char_path.is_file() || !user_path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let full = is_full(&dir);
        let (aspects, error) = match (load(&char_path), load(&user_path)) {
            (Ok(c), Ok(u)) => (derive_aspects(&c, &u, full), None),
            (Err(e), _) | (_, Err(e)) => (Vec::new(), Some(e)),
        };
        out.push(PresetInfo {
            name,
            dir: dir.to_string_lossy().into_owned(),
            char_path: char_path.to_string_lossy().into_owned(),
            user_path: user_path.to_string_lossy().into_owned(),
            modified_unix: modified_of([&char_path, &user_path]),
            aspects,
            full,
            error,
        });
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Decode one of a preset's documents.
pub fn load(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    blue_marshal::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a preset holds, read off the documents themselves. Adding a `Category`
/// and its `Aspect` is the only change a new aspect ever needs.
fn derive_aspects(char_doc: &Value, user_doc: &Value, full: bool) -> Vec<Aspect> {
    let mut out = Vec::new();
    if has_category(char_doc, Category::Layout) {
        out.push(Aspect::Layout);
    }
    if has_category(user_doc, Category::Overview) {
        out.push(Aspect::Overview);
    }
    if has_category(user_doc, Category::Autofill) {
        out.push(Aspect::Autofill);
    }
    if has_category(user_doc, Category::Keybinds) {
        out.push(Aspect::Keybinds);
    }
    if full {
        out.push(Aspect::Everything);
    }
    out
}

pub fn rename(app_data: &Path, old: &str, new: &str) -> Result<(), String> {
    let from = preset_path(app_data, old).map_err(|e| e.0)?;
    let to = preset_path(app_data, new).map_err(|e| e.0)?;
    if !from.is_dir() {
        return Err(format!("No preset called \u{201c}{old}\u{201d}."));
    }
    // `exists()` is case-insensitive on NTFS, so renaming "mining" to "Mining"
    // would otherwise report a collision with the very folder being renamed and
    // make fixing a preset's capitalisation impossible. Canonicalizing both
    // sides tells the two cases apart: the same directory resolves to the same
    // real path, while two genuinely distinct folders (possible on a
    // case-sensitive filesystem) do not.
    let same_folder = match (std::fs::canonicalize(&from), std::fs::canonicalize(&to)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if to.exists() && !same_folder {
        return Err(format!("A preset called \u{201c}{new}\u{201d} already exists."));
    }
    std::fs::rename(&from, &to).map_err(|e| format!("renaming the preset failed: {e}"))
}

pub fn delete(app_data: &Path, name: &str) -> Result<(), String> {
    let dir = preset_path(app_data, name).map_err(|e| e.0)?;
    if !dir.is_dir() {
        return Err(format!("No preset called \u{201c}{name}\u{201d}."));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("deleting the preset failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn accepts_ordinary_names() {
        assert_eq!(sanitize_name("PvP layout").unwrap(), "PvP layout");
        assert_eq!(sanitize_name("Mining — Ørca").unwrap(), "Mining — Ørca");
        assert_eq!(sanitize_name("v2.1 setup").unwrap(), "v2.1 setup");
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(sanitize_name("").is_err());
        assert!(sanitize_name("   ").is_err());
    }

    #[test]
    fn rejects_path_separators_and_wildcards() {
        for bad in ["a/b", "a\\b", "C:", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_control_characters() {
        assert!(sanitize_name("a\nb").is_err());
        assert!(sanitize_name("a\0b").is_err());
    }

    #[test]
    fn rejects_leading_or_trailing_dot_or_space() {
        // Windows strips these, so "foo." and "foo" would collide on disk.
        for bad in [".hidden", "trailing.", " lead", "trail ", ".", ".."] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_windows_reserved_device_names() {
        for bad in ["CON", "con", "NUL", "com1", "LPT9", "aux"] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
        // Only the stem matters to Windows, so an extension does not save it.
        assert!(sanitize_name("CON.txt").is_err());
        // But a name that merely starts with those letters is fine.
        assert!(sanitize_name("Console setup").is_ok());
    }

    #[test]
    fn rejects_the_backup_directory_name() {
        assert!(sanitize_name("eve-settings-editor-backups").is_err());
    }

    #[test]
    fn rejects_over_long_names() {
        assert!(sanitize_name(&"a".repeat(101)).is_err());
        assert!(sanitize_name(&"a".repeat(100)).is_ok());
    }

    #[test]
    fn preset_path_is_a_direct_child_of_the_presets_dir() {
        let root = Path::new("/data");
        let p = preset_path(root, "PvP layout").unwrap();
        assert_eq!(p, Path::new("/data").join("presets").join("PvP layout"));
    }

    #[test]
    fn preset_path_refuses_traversal_and_absolute_names() {
        // sanitize_name already rejects these; preset_path asserts containment
        // independently, so a future sanitiser gap still cannot escape.
        assert!(preset_path(Path::new("/data"), "../escape").is_err());
        assert!(preset_path(Path::new("/data"), "/etc/passwd").is_err());
        assert!(preset_path(Path::new("/data"), "a/b").is_err());
    }

    #[test]
    fn the_containment_guard_stands_alone() {
        // These never reach the guard through preset_path, because
        // sanitize_name rejects them first. Testing the guard directly is what
        // makes the second line of defence real rather than decorative.
        //
        // Only cases that behave the same on every platform belong here: CI is
        // Linux, where `\` and `:` are ordinary filename characters, so `a\b`
        // and `C:\Windows` ARE single components there. Both are already
        // covered as literal bad characters by rejects_path_separators_and_wildcards.
        for bad in ["..", ".", "a/b", "/etc/passwd", ""] {
            assert!(!is_single_normal_component(bad), "{bad:?} must not be a single component");
        }
        for good in ["PvP layout", "Mining", "v2.1 setup", "a/"] {
            assert!(is_single_normal_component(good), "{good:?} is a single component");
        }
    }

    use blue_marshal::{decode, Value};
    use settings_model::Category;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    fn temp_data(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eve-presets-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A char document with both char-side categories present.
    fn char_doc() -> Value {
        let windows = Value::Dict(vec![(b("openWindows"), Value::Dict(vec![(b("market"), Value::Bool(true))]))]);
        let sizes = Value::Dict(vec![(b("NAME"), Value::Int(120))]);
        let ui = Value::Dict(vec![(b("SortHeadersSizes"), Value::Tuple(vec![ts(), sizes]))]);
        Value::Dict(vec![(b("windows"), windows), (b("ui"), ui), (b("charStore"), Value::Int(7))])
    }

    /// A user document with all three account-side categories present.
    fn user_doc() -> Value {
        let overview = Value::Dict(vec![(b("overviewColumns"), Value::List(vec![b("NAME")]))]);
        let hist = Value::Dict(vec![(b("/search"), Value::List(vec![Value::Str("Jita".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        let cmds = Value::Dict(vec![(b("CmdToggleAutopilot"), Value::List(vec![Value::Int(65)]))]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), cmds]))]);
        Value::Dict(vec![(b("overview"), overview), (b("ui"), ui), (b("cmd"), cmd)])
    }

    fn read_doc(p: &Path) -> Value {
        decode(&std::fs::read(p).expect("preset file exists")).expect("preset file decodes")
    }

    #[test]
    fn a_layout_preset_carries_the_layout_and_nothing_else() {
        let data = temp_data("layout-only");
        let dir = create(
            &data,
            "Layout only",
            &[Aspect::Layout],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();

        let c = read_doc(&dir.join(CHAR_FILE));
        assert!(has_category(&c, Category::Layout), "the layout is there");
        // The privacy guarantee, asserted rather than implied.
        assert!(!has_category(&c, Category::OverviewWidths), "no overview widths leaked");
        let u = read_doc(&dir.join(USER_FILE));
        assert!(!has_category(&u, Category::Autofill), "no autofill history leaked");
        assert!(!has_category(&u, Category::Overview), "no overview leaked");
        assert!(!has_category(&u, Category::Keybinds), "no keybinds leaked");
        assert!(!dir.join(MARKER_FILE).exists(), "a pruned preset is not marked full");
    }

    #[test]
    fn a_pruned_document_keeps_no_unrelated_root_keys() {
        let data = temp_data("no-siblings");
        let dir = create(
            &data,
            "Tidy",
            &[Aspect::Layout],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        let Value::Dict(root) = read_doc(&dir.join(CHAR_FILE)) else { panic!("root is a dict") };
        // Exactly `windows` — not `charStore`, and no empty `ui` left behind.
        assert_eq!(root.len(), 1, "root holds one key, got {root:?}");
        assert_eq!(root[0].0, b("windows"));
    }

    #[test]
    fn an_autofill_preset_builds_its_parent_dict() {
        let data = temp_data("autofill");
        let dir = create(
            &data,
            "Autofill",
            &[Aspect::Autofill],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        let u = read_doc(&dir.join(USER_FILE));
        assert!(has_category(&u, Category::Autofill));
        assert!(!has_category(&u, Category::Overview));
        // The char side is a valid, empty document.
        assert_eq!(read_doc(&dir.join(CHAR_FILE)), Value::Dict(vec![]));
    }

    #[test]
    fn a_keybinds_preset_builds_the_cmd_parent() {
        let data = temp_data("keybinds");
        let dir = create(
            &data,
            "Keys",
            &[Aspect::Keybinds],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        let u = read_doc(&dir.join(USER_FILE));
        assert!(has_category(&u, Category::Keybinds), "keybinds needed a `cmd` parent");
        assert!(!has_category(&u, Category::Autofill));
    }

    #[test]
    fn an_overview_preset_spans_both_sides() {
        let data = temp_data("overview");
        let dir = create(
            &data,
            "Overview",
            &[Aspect::Overview],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        assert!(has_category(&read_doc(&dir.join(USER_FILE)), Category::Overview));
        assert!(has_category(&read_doc(&dir.join(CHAR_FILE)), Category::OverviewWidths));
    }

    #[test]
    fn everything_copies_both_documents_whole_and_marks_them() {
        let data = temp_data("everything");
        let dir = create(
            &data,
            "Full",
            &[Aspect::Everything],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        assert_eq!(read_doc(&dir.join(CHAR_FILE)), char_doc());
        assert_eq!(read_doc(&dir.join(USER_FILE)), user_doc());
        assert!(dir.join(MARKER_FILE).exists(), "a full preset is marked");
    }

    #[test]
    fn a_collision_is_refused_unless_overwrite() {
        let data = temp_data("collision");
        // Bind the documents first: a closure returning CreateInput built from
        // `&char_doc()` would borrow a temporary that dies with the expression.
        let (c, u) = (char_doc(), user_doc());
        let input = || CreateInput { char_doc: Some(&c), user_doc: Some(&u) };
        create(&data, "Same", &[Aspect::Layout], input(), false).unwrap();
        let err = create(&data, "Same", &[Aspect::Layout], input(), false).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
        create(&data, "Same", &[Aspect::Layout], input(), true).expect("overwrite is allowed");
    }

    #[test]
    fn an_aspect_whose_side_is_not_open_is_refused() {
        let data = temp_data("missing-side");
        let err = create(
            &data,
            "No account",
            &[Aspect::Overview],
            CreateInput { char_doc: Some(&char_doc()), user_doc: None },
            false,
        )
        .unwrap_err();
        assert!(err.contains("account file"), "got: {err}");
        // Nothing was written.
        assert!(!preset_path(&data, "No account").unwrap().exists());
    }

    #[test]
    fn overwriting_a_full_preset_with_a_pruned_one_drops_the_marker() {
        let data = temp_data("full-to-pruned");
        let (c, u) = (char_doc(), user_doc());
        let input = || CreateInput { char_doc: Some(&c), user_doc: Some(&u) };
        create(&data, "Switch", &[Aspect::Everything], input(), false).unwrap();
        let dir = preset_path(&data, "Switch").unwrap();
        assert!(dir.join(MARKER_FILE).exists(), "the full preset is marked");

        create(&data, "Switch", &[Aspect::Layout], input(), true).unwrap();
        assert!(!dir.join(MARKER_FILE).exists(), "a pruned overwrite must drop the marker");
        // And the documents really were replaced, not left as the full copy.
        assert!(!has_category(&read_doc(&dir.join(USER_FILE)), Category::Autofill));
    }

    fn make(data: &Path, name: &str, aspects: &[Aspect]) {
        create(
            data,
            name,
            aspects,
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
    }

    #[test]
    fn list_derives_each_presets_aspects() {
        let data = temp_data("list");
        make(&data, "Just layout", &[Aspect::Layout]);
        make(&data, "Layout and keys", &[Aspect::Layout, Aspect::Keybinds]);
        make(&data, "The lot", &[Aspect::Everything]);

        let all = list(&data);
        let by = |n: &str| all.iter().find(|p| p.name == n).unwrap_or_else(|| panic!("{n} listed"));

        assert_eq!(by("Just layout").aspects, vec![Aspect::Layout]);
        assert!(!by("Just layout").full);
        assert_eq!(by("Layout and keys").aspects, vec![Aspect::Layout, Aspect::Keybinds]);
        // A full preset holds every aspect AND is marked.
        let lot = by("The lot");
        assert!(lot.full);
        assert!(lot.aspects.contains(&Aspect::Everything));
        assert!(lot.aspects.contains(&Aspect::Overview));
        assert!(all.iter().all(|p| p.error.is_none()));
    }

    #[test]
    fn list_is_sorted_by_name_case_insensitively() {
        let data = temp_data("sorted");
        // "banana" vs "Cherry" discriminates: uppercase ASCII sorts BELOW
        // lowercase, so a naive byte sort would put "Cherry" first. Only a
        // case-folded key gets this order, which is what the assertion pins.
        make(&data, "banana", &[Aspect::Layout]);
        make(&data, "Cherry", &[Aspect::Layout]);
        let names: Vec<String> = list(&data).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["banana".to_string(), "Cherry".to_string()]);
    }

    #[test]
    fn a_missing_marker_reads_as_pruned() {
        let data = temp_data("marker-gone");
        make(&data, "Full", &[Aspect::Everything]);
        std::fs::remove_file(preset_path(&data, "Full").unwrap().join(MARKER_FILE)).unwrap();
        let p = list(&data).into_iter().find(|p| p.name == "Full").unwrap();
        // Safe direction: fewer aspects offered, never a destructive full copy.
        assert!(!p.full);
        assert!(!p.aspects.contains(&Aspect::Everything));
    }

    #[test]
    fn an_undecodable_preset_is_listed_with_an_error() {
        let data = temp_data("broken");
        make(&data, "Broken", &[Aspect::Layout]);
        std::fs::write(preset_path(&data, "Broken").unwrap().join(CHAR_FILE), b"not marshal").unwrap();
        let p = list(&data).into_iter().find(|p| p.name == "Broken").unwrap();
        assert!(p.error.is_some(), "a broken preset must say so, not vanish");
        assert!(p.aspects.is_empty());
    }

    #[test]
    fn list_ignores_stray_files_and_the_backup_dir() {
        let data = temp_data("strays");
        make(&data, "Real", &[Aspect::Layout]);
        std::fs::write(presets_dir(&data).join("loose.txt"), b"x").unwrap();
        std::fs::create_dir_all(presets_dir(&data).join("Not a preset")).unwrap();
        let names: Vec<String> = list(&data).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Real".to_string()], "only folders with both documents count");
    }

    #[test]
    fn list_of_a_missing_directory_is_empty_not_an_error() {
        let data = temp_data("never-used");
        assert!(list(&data).is_empty());
        assert!(!presets_dir(&data).exists(), "listing must not create the directory");
    }

    #[test]
    fn rename_moves_the_folder_and_refuses_a_collision() {
        let data = temp_data("rename");
        make(&data, "Old", &[Aspect::Layout]);
        make(&data, "Taken", &[Aspect::Layout]);
        rename(&data, "Old", "New").unwrap();
        assert!(preset_path(&data, "New").unwrap().exists());
        assert!(!preset_path(&data, "Old").unwrap().exists());
        assert!(rename(&data, "New", "Taken").is_err(), "collision refused");
        assert!(rename(&data, "New", "../escape").is_err(), "traversal refused");
    }

    #[test]
    fn a_case_only_rename_fixes_the_capitalisation() {
        let data = temp_data("case-rename");
        make(&data, "mining", &[Aspect::Layout]);
        // On NTFS the destination "exists" because it IS the folder being
        // renamed, so a naive collision guard would make fixing a preset's
        // capitalisation impossible. On a case-sensitive filesystem this is
        // simply a rename to a free name. Both must succeed.
        rename(&data, "mining", "Mining").expect("capitalisation must be fixable");
        let names: Vec<String> = list(&data).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Mining".to_string()], "one preset, newly capitalised");
    }

    #[test]
    fn delete_removes_the_folder_and_refuses_traversal() {
        let data = temp_data("delete");
        make(&data, "Doomed", &[Aspect::Layout]);
        delete(&data, "Doomed").unwrap();
        assert!(!preset_path(&data, "Doomed").unwrap().exists());
        assert!(delete(&data, "../escape").is_err());
        assert!(delete(&data, "Doomed").is_err(), "deleting a missing preset is an error");
    }
}
