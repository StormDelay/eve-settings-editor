//! The preset library: a folder of presets, each holding a char-side and an
//! account-side settings document. Nothing here invents a file format — a
//! preset's two files are ordinary settings documents, which is exactly what
//! lets the editors open one as if it were a character.

use std::path::{Component, Path, PathBuf};

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

use blue_marshal::{encode, Value};
use settings_model::{apply_to_tree, extract_categories, Category};

use crate::ops::{aspect_writes, Aspect};

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
    // Encode BEFORE creating anything, so a failure leaves no half-written
    // preset behind.
    let char_bytes = encode(&side(docs.char_doc, &w.char_categories))
        .map_err(|e| format!("encoding the character side failed: {e}"))?;
    let user_bytes = encode(&side(docs.user_doc, &w.account_categories))
        .map_err(|e| format!("encoding the account side failed: {e}"))?;

    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("replacing the preset failed: {e}"))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating the preset failed: {e}"))?;
    std::fs::write(dir.join(CHAR_FILE), &char_bytes).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(USER_FILE), &user_bytes).map_err(|e| e.to_string())?;
    if full {
        std::fs::write(dir.join(MARKER_FILE), br#"{"full":true}"#).map_err(|e| e.to_string())?;
    }
    Ok(dir)
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
}
