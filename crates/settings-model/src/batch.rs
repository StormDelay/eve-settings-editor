//! Batch apply: extract a projection category's subtree from one document and
//! splice it into another. The category subtree is the VALUE at a fixed key
//! path — `windows` (char file) or `ui -> editHistory` (user file). Extract
//! inlines the source's sharing first so a Ref inside the category that points
//! at a Shared defined elsewhere resolves; splice inlines the target's sharing
//! first so replacing the subtree can never dangle a Ref the rest of the file
//! still holds (the proven autofill.rs / overview.rs inline-first idiom).

use blue_marshal::Value;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::document::{Document, LoadError};
use crate::save::{save, SaveReport};
use crate::treewalk::{inline_all, is_bytes, Entries};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Layout,
    Autofill,
    Overview,
    OverviewWidths,
    Keybinds,
    /// Custom probe scanner formations, account-side. The dot in
    /// `probescanning.customFormations` is part of the key NAME, so this is a
    /// two-level path under `ui`, not three levels through a `probescanning`
    /// section (probes.rs, spec §2.1).
    ///
    /// Two deliberate disagreements with the editor path in `probes.rs`, both
    /// harmless on corpus data:
    /// - This category copies the formations dict but never
    ///   `selectedFormationID`, so a copy can leave the target's selection
    ///   naming a formation that no longer exists. `probes.rs::remove_formation`
    ///   goes out of its way to prevent exactly that state on its own path,
    ///   calling it "the one outcome that could confuse the client" — the two
    ///   paths disagree about how much it matters. Near-zero in practice: the
    ///   corpus's `selectedFormationID` is `0` everywhere.
    /// - A whole-section splice overwrites the target's `-4` scratch slot,
    ///   which `probes.rs` never touches on any write path by explicit design.
    ///   Harmless, since the client regenerates it.
    ProbeFormations,
    NeocomButtons,
    // The HUD's individual keys. `hud.rs`'s FIELDS table is the source of
    // truth for these paths; `Aspect::Layout` carries all of them so a layout
    // copy moves the whole of a character's screen furniture. They cannot be
    // whole-section splices: char `ui` also holds editHistory and
    // SortHeadersSizes, so copying the section would carry the target's
    // autofill away.
    HudFighterPos,
    HudBadge,
    HudShipTop,
    HudFighterDetached,
    HudFighterShown,
    HudNeocomWidth,
    // The locked-target list: where it sits, and which way it runs. Its
    // `targetOriginLocked` sibling is deliberately NOT carried — that is a
    // per-player interaction preference, and copying a locked state onto
    // someone would leave them unable to drag their own list back in-game.
    HudTargetOrigin,
    HudTargetAlign,
}

impl Category {
    /// Key path from the document root to this category's subtree VALUE.
    /// Public so a caller building a document that holds only some categories
    /// can create exactly the intermediate parent dicts they need — see the
    /// app crate's `presets::prune`.
    pub fn key_path(self) -> &'static [&'static [u8]] {
        match self {
            Category::Layout => &[b"windows"],
            Category::Autofill => &[b"ui", b"editHistory"],
            Category::Overview => &[b"overview"],
            Category::OverviewWidths => &[b"ui", b"SortHeadersSizes"],
            Category::Keybinds => &[b"cmd", b"customCmds"],
            Category::ProbeFormations => &[b"ui", b"probescanning.customFormations"],
            // Character-side: the neocom BAR is per account (neocomWidth), its
            // BUTTONS are per character. Original is deliberately not a category
            // — it is the target's own client baseline.
            Category::NeocomButtons => &[b"ui", b"neocomButtonRawData"],
            Category::HudFighterPos => &[b"ui", b"fightersDetachedPosition"],
            Category::HudBadge => &[b"notifications", b"notification_badge_offset"],
            Category::HudShipTop => &[b"ui", b"shipuialigntop"],
            Category::HudFighterDetached => &[b"ui", b"detachFighterUI"],
            Category::HudFighterShown => &[b"ui", b"displayFighterUI"],
            // Account-side `windows`, which holds only this key — a different
            // document from the char-side `windows` Category::Layout splices.
            Category::HudNeocomWidth => &[b"windows", b"neocomWidth"],
            // Account-side `ui`, like the three toggles above it. NOT
            // `windows`: a plain `bmdump dump` walks these two back there, and
            // only an inlined dump shows the real tree. See hud.rs.
            Category::HudTargetOrigin => &[b"ui", b"targetOrigin"],
            Category::HudTargetAlign => &[b"ui", b"alignHorizontally"],
        }
    }

    /// Whether an absent key on the SOURCE means "EVE's default" rather than
    /// "nothing to copy". True only for the leaf HUD keys: 851 of 3059 corpus
    /// account files store none of them, so treating absence as "leave the
    /// target alone" would half-apply a Layout copy on a quarter of accounts.
    /// Never true for a whole-section category — a source with no `overview`
    /// deleting the target's would be data loss, not a copy.
    ///
    /// **`HudTargetOrigin` makes this path common rather than occasional.**
    /// 87 % of corpus account files have never had the target list dragged, so
    /// most Layout copies now DELETE the target's target-list position, putting
    /// it back at EVE's default. That is the same rule the other leaves follow
    /// and it is what "the two characters match" means — but where the others
    /// fire on a quarter of sources, this one fires on most of them, and the
    /// target's own carefully-placed list is what goes. Worth saying out loud
    /// before anyone reads the deletion as a bug.
    ///
    /// One case is genuinely lossy and has no signal to catch it: a Layout
    /// preset saved between 0.23.0 and 0.26.0 has a non-empty `user.dat` (so
    /// the empty-root rule below does not apply) and no `targetOrigin` (because
    /// the category did not exist), and applying it therefore deletes the
    /// target's. The char side has a shape signal for exactly this — `prune`
    /// building a root `notifications` key — but these two live under `ui`,
    /// which every such preset already has, so there is nothing to test. Filed
    /// in docs/small-tasks.md; re-saving the preset fixes it.
    pub fn absent_means_default(self) -> bool {
        matches!(
            self,
            Category::HudFighterPos
                | Category::HudBadge
                | Category::HudShipTop
                | Category::HudFighterDetached
                | Category::HudFighterShown
                | Category::HudNeocomWidth
                | Category::HudTargetOrigin
                | Category::HudTargetAlign
        )
    }
}

/// Whether an absent `cat` in THIS source document may be read as "the source
/// sits at EVE's default" — the only thing that licenses `apply_to_tree` to
/// DELETE that key from the target, which is the one place in the app that
/// removes anything from a player's settings file. It is
/// `cat.absent_means_default()` (the per-category half) plus one per-document
/// shape check on the char side.
///
/// THE SIGNAL. A Layout preset created after the aspect grew its HUD
/// categories always has a `notifications` root key in its `char.dat`, because
/// `presets::prune` builds that parent for `Category::HudBadge` whether or not
/// the source stores `notification_badge_offset`. A Layout preset created
/// BEFORE never has one: its `char.dat` holds `windows` and
/// `ui -> neocomButtonRawData`, and nothing else. So a source document with no
/// `notifications` root key predates the HUD-carrying Layout aspect. Its
/// missing `fightersDetachedPosition` and `notification_badge_offset` were
/// never captured rather than captured-as-default, and deleting them would
/// silently lose the TARGET character's fighter-panel and badge positions — on
/// a preset the user saved before this behaviour existed and never re-captured.
///
/// WHY IT CANNOT MISFIRE ON A REAL CHARACTER FILE. EVE writes a root
/// `notifications` section into every character file — all 6502 in the corpus
/// carry it (`tests/hud_corpus.rs::a_real_char_file_is_never_read_as_a_pre_hud_preset`
/// asserts exactly this, through this function). A real character source
/// therefore keeps full removal semantics.
///
/// WHAT WOULD SILENTLY BREAK IT. Dropping `Category::HudBadge` from
/// `Aspect::Layout`'s char-side category list: `prune` would stop building the
/// `notifications` parent, every newly created Layout preset would then look
/// like an old one, and the char-side HUD would quietly stop copying its
/// defaults. `presets.rs`'s
/// `a_new_layout_preset_carries_the_notifications_shape_signal` fails loudly
/// if that ever happens.
///
/// A source that is itself old-shaped (an old preset's `char.dat` re-opened in
/// the char slot and cut into a new preset) produces another old-shaped preset.
/// That is the safe direction — it removes nothing — so it is left alone.
///
/// The ACCOUNT side is discriminated differently, by `extract_categories`'s
/// empty-root check: a pre-branch Layout preset's `user.dat` IS `{}`. This rule
/// must never be extended to account categories — a real account file has no
/// `notifications` section at all, so it would disable every account-side
/// removal.
fn absence_means_eve_default(root: &Entries, cat: Category) -> bool {
    if !cat.absent_means_default() {
        return false;
    }
    match cat {
        Category::HudFighterPos | Category::HudBadge => {
            root.iter().any(|(k, _)| is_bytes(k, b"notifications"))
        }
        _ => true,
    }
}

/// Inline the source's sharing, then clone each requested category's subtree.
/// A category the source lacks is skipped — EXCEPT one whose absence
/// `absence_means_eve_default` accepts, which is returned as `(cat, None)` so
/// the splice removes the target's own value. An absent leaf HUD key is EVE's
/// default, not "nothing to copy" — as long as the source is a document that
/// could have stored it (see that function).
pub fn extract_categories(source: &Value, cats: &[Category]) -> Vec<(Category, Option<Value>)> {
    let mut s = source.clone();
    inline_all(&mut s);
    let Value::Dict(root) = &s else { return Vec::new() };
    // An empty root is a preset side that was pruned away, never a real
    // settings file. It holds no values AND claims no absences: a Layout
    // preset created before the aspect grew an account side must not delete
    // the target's HUD keys. See the spec's §4.4. This covers the ACCOUNT side
    // only — an old preset's char.dat is not empty, which is what
    // `absence_means_eve_default` above is for.
    if root.is_empty() {
        return Vec::new();
    }
    cats.iter()
        .filter_map(|&cat| {
            let keys = cat.key_path();
            let (parent_keys, last) = keys.split_at(keys.len() - 1);
            let found = descend_ref(root, parent_keys)
                .and_then(|parent| parent.iter().find(|(k, _)| is_bytes(k, last[0])))
                .map(|(_, v)| v.clone());
            match found {
                Some(v) => Some((cat, Some(v))),
                None if absence_means_eve_default(root, cat) => Some((cat, None)),
                None => None,
            }
        })
        .collect()
}

/// Inline the target's sharing, then replace (or insert) each category's
/// subtree — or REMOVE it, for a `None` (see `extract_categories`).
/// A missing intermediate parent dict (e.g. no `ui`) skips that category.
pub fn apply_to_tree(target: &mut Value, extracted: &[(Category, Option<Value>)]) {
    inline_all(target);
    if let Value::Dict(root) = target {
        for (cat, subtree) in extracted {
            let keys = cat.key_path();
            let (parent_keys, last) = keys.split_at(keys.len() - 1);
            let Some(parent) = descend_mut(root, parent_keys) else { continue };
            match subtree {
                // The source is at EVE's default, so the target's own value
                // has to go — leaving it would half-apply the copy.
                None => parent.retain(|(k, _)| !is_bytes(k, last[0])),
                Some(subtree) => match parent.iter_mut().find(|(k, _)| is_bytes(k, last[0])) {
                    Some((_, v)) => *v = subtree.clone(),
                    None => parent.push((Value::Bytes(last[0].to_vec()), subtree.clone())),
                },
            }
        }
    }
    // Re-derive compact immutable-only sharing so the saved file is not the
    // ~1.5x fully-inlined blob (no reliance on EVE re-deduplicating).
    *target = blue_marshal::reshare(target);
}

/// Back up `target`, then atomically overwrite it with `source_bytes`. Byte-for-
/// byte; the source is already a valid file. Returns the backup path.
pub fn full_copy_to(source_bytes: &[u8], target: &Path) -> Result<PathBuf, String> {
    let backup = crate::save::backup_current(target)?;
    crate::save::atomic_write(target, source_bytes)?;
    Ok(backup)
}

/// Load `target`, splice each extracted category in, and run the full save chain
/// (encode -> verify -> backup -> atomic write; ReadOnly targets are refused).
/// `force_conflict = true`: the target is loaded fresh in this call, so there is
/// no genuine conflict to guard against.
pub fn apply_categories_to(
    target: &Path,
    extracted: &[(Category, Option<Value>)],
) -> Result<SaveReport, String> {
    let mut doc = Document::load(target).map_err(|e| match e {
        LoadError::Io(m) => format!("Io: {m}"),
        LoadError::Decode { message, .. } => format!("Decode: {message}"),
    })?;
    apply_to_tree(&mut doc.value, extracted);
    save(&mut doc, true).map_err(|e| format!("{e:?}"))
}

/// Inner dict of a plain (post-inline) value, unwrapping a `(ts, dict)` tuple.
fn dict_inner(v: &Value) -> Option<&Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

fn dict_inner_mut(v: &mut Value) -> Option<&mut Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

fn descend_ref<'a>(root: &'a Entries, keys: &[&[u8]]) -> Option<&'a Entries> {
    let mut cur = root;
    for &key in keys {
        let (_, v) = cur.iter().find(|(k, _)| is_bytes(k, key))?;
        cur = dict_inner(v)?;
    }
    Some(cur)
}

fn descend_mut<'a>(root: &'a mut Entries, keys: &[&[u8]]) -> Option<&'a mut Entries> {
    let mut cur = root;
    for &key in keys {
        let (_, v) = cur.iter_mut().find(|(k, _)| is_bytes(k, key))?;
        cur = dict_inner_mut(v)?;
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{decode, encode};

    #[test]
    fn the_probe_formation_category_is_a_two_level_ui_key() {
        // The dot is part of the key NAME. A three-level path through a
        // `probescanning` section finds nothing and silently copies nothing.
        assert_eq!(
            Category::ProbeFormations.key_path(),
            &[b"ui".as_slice(), b"probescanning.customFormations".as_slice()],
        );
    }

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    /// user root -> ui -> editHistory -> (ts, { "/a": ["Jita"] })
    fn user_a() -> Value {
        let hist = Value::Dict(vec![(b("/a"), Value::List(vec![Value::Str("Jita".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui)])
    }

    /// user root -> ui -> editHistory -> (ts, { "/b": ["Amarr"] }) plus a sibling key.
    fn user_b() -> Value {
        let hist = Value::Dict(vec![(b("/b"), Value::List(vec![Value::Str("Amarr".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui), (b("keep"), Value::Int(7))])
    }

    #[test]
    fn extract_then_apply_replaces_the_category_and_keeps_siblings() {
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        assert_eq!(extracted.len(), 1);
        let mut target = user_b();
        apply_to_tree(&mut target, &extracted);

        // The autofill category is now A's; the unrelated sibling survived.
        let lists = crate::autofill::project_edit_history(&target);
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].widget, "/a");
        assert_eq!(lists[0].entries, vec!["Jita"]);
        let Value::Dict(root) = &target else { panic!() };
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"keep") && matches!(v, Value::Int(7))));
    }

    #[test]
    fn apply_inserts_the_category_when_the_target_lacks_it() {
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        // Target has a `ui` dict but no editHistory entry.
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);
        let lists = crate::autofill::project_edit_history(&target);
        assert_eq!(lists[0].entries, vec!["Jita"]);
    }

    #[test]
    fn extract_resolves_a_ref_into_a_shared_defined_outside_the_category() {
        // The category's list holds a Ref; the Shared it points at is defined
        // OUTSIDE editHistory. Without inlining the whole source first, the
        // extracted subtree would carry a dangling Ref that fails to encode.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let hist = Value::Dict(vec![(b("/a"), Value::List(vec![Value::Ref(1)]))]);
        let ui = Value::Dict(vec![
            (b("shareDef"), Value::List(vec![jita])), // Shared def, sibling of editHistory
            (b("editHistory"), Value::Tuple(vec![ts(), hist])),
        ]);
        let source = Value::Dict(vec![(b("ui"), ui)]);
        encode(&source).expect("fixture encodes (def precedes ref)");

        let extracted = extract_categories(&source, &[Category::Autofill]);
        // Put the extracted subtree in a bare target and prove it encodes alone.
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);
        let bytes = encode(&target).expect("extracted subtree has no dangling Ref");
        let lists = crate::autofill::project_edit_history(&decode(&bytes).unwrap());
        assert_eq!(lists[0].entries, vec!["Jita"]);
    }

    #[test]
    fn apply_inlines_the_target_so_an_outside_ref_into_the_old_category_survives() {
        // Target: the OLD editHistory holds a Shared def; a sibling Ref points at
        // it. Replacing editHistory drops the def — so apply_to_tree must inline
        // the target first or the sibling Ref dangles on encode.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let old_hist = Value::Dict(vec![(b("/old"), Value::List(vec![jita]))]);
        let ui = Value::Dict(vec![
            (b("editHistory"), Value::Tuple(vec![ts(), old_hist])), // def, encoded first
            (b("sibling"), Value::List(vec![Value::Ref(1)])),       // ref outside the category
        ]);
        let mut target = Value::Dict(vec![(b("ui"), ui)]);
        encode(&target).expect("target fixture encodes before the splice");

        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        apply_to_tree(&mut target, &extracted);
        encode(&target).expect("post-splice target encodes (outside Ref inlined, not dangled)");
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("batch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn full_copy_overwrites_bytes_and_backs_up() {
        let dir = temp_dir("full");
        let src = dir.join("core_char_1.dat");
        let dst = dir.join("core_char_2.dat");
        let src_bytes = encode(&user_a()).unwrap();
        let dst_bytes = encode(&user_b()).unwrap();
        std::fs::write(&src, &src_bytes).unwrap();
        std::fs::write(&dst, &dst_bytes).unwrap();

        let backup = full_copy_to(&src_bytes, &dst).unwrap();
        assert!(backup.exists(), "target backed up before overwrite");
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            dst_bytes,
            "backup captured target's pre-overwrite bytes"
        );
        assert_eq!(std::fs::read(&dst).unwrap(), src_bytes, "target now byte-identical to source");
    }

    #[test]
    fn category_apply_replaces_only_the_category_on_disk() {
        let dir = temp_dir("cat");
        let dst = dir.join("core_user_2.dat");
        std::fs::write(&dst, encode(&user_b()).unwrap()).unwrap();

        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        let report = apply_categories_to(&dst, &extracted).unwrap();
        assert!(report.backup_path.exists());

        let reread = decode(&std::fs::read(&dst).unwrap()).unwrap();
        let lists = crate::autofill::project_edit_history(&reread);
        assert_eq!(lists[0].widget, "/a", "category came from the source");
        let Value::Dict(root) = &reread else { panic!() };
        assert!(root.iter().any(|(k, _)| is_bytes(k, b"keep")), "sibling key preserved on disk");
    }

    #[test]
    fn category_apply_refuses_a_read_only_target() {
        // A non-canonical stream (INT8-encoded 1) loads ReadOnly; save refuses it.
        let dir = temp_dir("ro");
        let dst = dir.join("core_user_3.dat");
        std::fs::write(&dst, [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap();
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        let err = apply_categories_to(&dst, &extracted).unwrap_err();
        assert!(err.contains("ReadOnly"), "read-only target surfaced as an error: {err}");
    }

    /// user root -> overview -> { overviewColumns: ["NAME"], tabsByWindowInstanceID: [[0]] }
    fn user_overview(col: &str) -> Value {
        let overview = Value::Dict(vec![
            (b("overviewColumns"), Value::List(vec![b(col)])),
            (b("tabsByWindowInstanceID"), Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]);
        Value::Dict(vec![(b("overview"), overview), (b("keep"), Value::Int(7))])
    }

    /// char root -> ui -> SortHeadersSizes -> (ts, { (overviewScroll2, 0): { NAME: w } })
    fn char_widths(w: i64) -> Value {
        let cols = Value::Dict(vec![(b("NAME"), Value::Int(w))]);
        let sizes = Value::Dict(vec![(
            Value::Tuple(vec![b("overviewScroll2"), Value::Int(0)]),
            cols,
        )]);
        let ui = Value::Dict(vec![(b("SortHeadersSizes"), Value::Tuple(vec![ts(), sizes]))]);
        Value::Dict(vec![(b("ui"), ui), (b("other"), Value::Int(9))])
    }

    #[test]
    fn overview_category_replaces_the_overview_subtree_and_keeps_siblings() {
        let extracted = extract_categories(&user_overview("SOURCECOL"), &[Category::Overview]);
        assert_eq!(extracted.len(), 1);
        let mut target = user_overview("TARGETCOL");
        apply_to_tree(&mut target, &extracted);

        // The overview subtree is now the source's: overviewColumns == ["SOURCECOL"].
        let Value::Dict(root) = &target else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_bytes(k, b"overview")).unwrap();
        let Value::Dict(ov) = ov else { panic!() };
        let (_, cols) = ov.iter().find(|(k, _)| is_bytes(k, b"overviewColumns")).unwrap();
        assert_eq!(cols, &Value::List(vec![b("SOURCECOL")]), "overview came from the source");
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"keep") && matches!(v, Value::Int(7))),
            "unrelated sibling survived");
    }

    #[test]
    fn overview_widths_category_replaces_sortheaderssizes_and_keeps_siblings() {
        let extracted = extract_categories(&char_widths(120), &[Category::OverviewWidths]);
        assert_eq!(extracted.len(), 1);
        let mut target = char_widths(999);
        apply_to_tree(&mut target, &extracted);

        // The width came from the source: NAME == 120, not the target's 999.
        let Value::Dict(root) = &target else { panic!() };
        let (_, ui) = root.iter().find(|(k, _)| is_bytes(k, b"ui")).unwrap();
        let Value::Dict(ui) = ui else { panic!() };
        let (_, shs) = ui.iter().find(|(k, _)| is_bytes(k, b"SortHeadersSizes")).unwrap();
        let Value::Tuple(items) = shs else { panic!() };
        let Value::Dict(sizes) = &items[1] else { panic!() };
        let Value::Dict(cols) = &sizes[0].1 else { panic!() };
        assert_eq!(cols.iter().find(|(k, _)| is_bytes(k, b"NAME")).unwrap().1, Value::Int(120));
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"other") && matches!(v, Value::Int(9))),
            "sibling under root survived");
    }

    #[test]
    fn apply_to_tree_leaves_a_compact_shared_result() {
        use blue_marshal::encode;
        // A source Layout subtree whose window-id byte-string repeats across the
        // geometry + flag dicts (the real shape). After splicing into a target and
        // resharing, the encoded stream must carry shared objects (count > 0) and be
        // smaller than the fully-inlined encoding.
        let id = || Value::Bytes(b"overview_window".to_vec());
        let windows = Value::Dict(vec![
            (Value::Bytes(b"openWindows".to_vec()), Value::Dict(vec![(id(), Value::Bool(true))])),
            (Value::Bytes(b"lockedWindows".to_vec()), Value::Dict(vec![(id(), Value::Bool(false))])),
            (Value::Bytes(b"stacksWindows".to_vec()), Value::Dict(vec![(id(), id())])),
        ]);
        let extracted = vec![(Category::Layout, Some(windows))];

        let mut target = Value::Dict(vec![(Value::Bytes(b"windows".to_vec()), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);

        let bytes = encode(&target).expect("resharded target encodes");
        let shared_count = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        assert!(shared_count > 0, "reshare shared the repeated id, count={shared_count}");

        // Smaller than if we had left it fully inlined.
        let inlined_len = encode(&blue_marshal::inline(&target)).unwrap().len();
        assert!(bytes.len() < inlined_len, "{} !< {}", bytes.len(), inlined_len);
    }

    /// `cmd -> customCmds` is the same two-step path shape as Autofill's
    /// `ui -> editHistory`, so extract/apply need no new machinery.
    #[test]
    fn keybinds_category_round_trips_the_whole_table() {
        let ts = || Value::Long(vec![0u8; 8]);
        let bts = |s: &str| Value::Bytes(s.as_bytes().to_vec());
        let table = |code: i64| {
            Value::Dict(vec![(
                bts("customCmds"),
                Value::Tuple(vec![
                    ts(),
                    Value::Dict(vec![(bts("CmdApproachItem"), Value::Tuple(vec![Value::Int(code)]))]),
                ]),
            )])
        };
        let source = Value::Dict(vec![(bts("cmd"), table(65))]);
        let mut target = Value::Dict(vec![(bts("cmd"), table(90))]);

        let extracted = extract_categories(&source, &[Category::Keybinds]);
        assert_eq!(extracted.len(), 1);
        apply_to_tree(&mut target, &extracted);

        let binds = settings_model_project(&target);
        assert_eq!(binds, Some(vec![65]), "the source's binding replaced the target's");
    }

    /// The load-bearing case: an account whose table is EMPTY gets one.
    #[test]
    fn keybinds_category_populates_an_empty_table() {
        let ts = || Value::Long(vec![0u8; 8]);
        let bts = |s: &str| Value::Bytes(s.as_bytes().to_vec());
        let source = Value::Dict(vec![(
            bts("cmd"),
            Value::Dict(vec![(
                bts("customCmds"),
                Value::Tuple(vec![
                    ts(),
                    Value::Dict(vec![(bts("CmdApproachItem"), Value::Tuple(vec![Value::Int(65)]))]),
                ]),
            )]),
        )]);
        let mut target = Value::Dict(vec![(
            bts("cmd"),
            Value::Dict(vec![(bts("customCmds"), Value::Tuple(vec![ts(), Value::Dict(vec![])]))]),
        )]);

        apply_to_tree(&mut target, &extract_categories(&source, &[Category::Keybinds]));
        assert_eq!(settings_model_project(&target), Some(vec![65]));
    }

    /// Local helper: read CmdApproachItem's codes back out of a tree.
    fn settings_model_project(v: &Value) -> Option<Vec<i64>> {
        let k = crate::keybinds::project_keybinds(Some(v));
        let e = k.entries.iter().find(|e| e.command == "CmdApproachItem")?;
        e.keys.clone()
    }

    #[test]
    fn neocom_buttons_extract_and_apply_across_files() {
        let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![b("SOURCE-BAR")])])),
            (b("neocomButtonRawDataOriginal"), Value::Tuple(vec![ts(), Value::Tuple(vec![b("SOURCE-ORIGINAL")])])),
        ]))]);
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![
            (b("neocomButtonRawData"), Value::Tuple(vec![ts(), Value::List(vec![b("TARGET-BAR")])])),
            (b("neocomButtonRawDataOriginal"), Value::Tuple(vec![ts(), Value::Tuple(vec![b("TARGET-ORIGINAL")])])),
        ]))]);

        let extracted = extract_categories(&source, &[Category::NeocomButtons]);
        apply_to_tree(&mut target, &extracted);

        // `Value::Bytes` derives a plain-number `Debug` (no ASCII rendering), so a
        // debug-string-contains check can never see "SOURCE-BAR" — navigate the
        // tree and compare values directly instead, as the Overview tests above do.
        let Value::Dict(root) = &target else { panic!() };
        let (_, ui) = root.iter().find(|(k, _)| is_bytes(k, b"ui")).unwrap();
        let Value::Dict(ui) = ui else { panic!() };

        let (_, bar) = ui.iter().find(|(k, _)| is_bytes(k, b"neocomButtonRawData")).unwrap();
        let Value::Tuple(bar_items) = bar else { panic!() };
        assert_eq!(bar_items[1], Value::List(vec![b("SOURCE-BAR")]),
            "the source bar did not replace the target's");

        // The baseline is the TARGET's own client record: copying the source's
        // would corrupt what "reset to original" means on that character.
        let (_, original) = ui.iter().find(|(k, _)| is_bytes(k, b"neocomButtonRawDataOriginal")).unwrap();
        let Value::Tuple(orig_items) = original else { panic!() };
        assert_eq!(orig_items[1], Value::Tuple(vec![b("TARGET-ORIGINAL")]),
            "the target's Original was overwritten or the source's Original leaked across");
    }

    #[test]
    fn the_hud_categories_address_the_keys_hud_rs_writes() {
        // Exactly the paths in hud.rs's FIELDS table, which is the only other
        // place these keys are named. A drift here half-applies a Layout copy
        // silently, which is the bug this whole branch exists to fix.
        //
        // These literals are COPIES of FIELDS (which is private), so this pin
        // on its own only holds batch.rs against itself: change both and it
        // still passes. The genuine cross-check is
        // `ops.rs::a_layout_copy_leaves_every_hud_field_equal`, which copies
        // through these key paths and then reads every field back through
        // `project_hud` — the only reader of FIELDS. If hud.rs moved a section
        // or key without moving it here, the copy would write the old path and
        // the projection would read the new one, so the field comes back None
        // and that test fails. Keep it non-vacuous (every field non-None on
        // both sides) or this pair stops cross-checking anything.
        let expected: [(Category, &[&[u8]]); 8] = [
            (Category::HudFighterPos, &[b"ui", b"fightersDetachedPosition"]),
            (Category::HudBadge, &[b"notifications", b"notification_badge_offset"]),
            (Category::HudShipTop, &[b"ui", b"shipuialigntop"]),
            (Category::HudFighterDetached, &[b"ui", b"detachFighterUI"]),
            (Category::HudFighterShown, &[b"ui", b"displayFighterUI"]),
            (Category::HudNeocomWidth, &[b"windows", b"neocomWidth"]),
            // `ui`, not `windows` — the trap hud.rs documents at length.
            (Category::HudTargetOrigin, &[b"ui", b"targetOrigin"]),
            (Category::HudTargetAlign, &[b"ui", b"alignHorizontally"]),
        ];
        for (cat, path) in expected {
            assert_eq!(cat.key_path(), path, "{cat:?} addresses the wrong key");
            assert!(cat.absent_means_default(), "{cat:?} is a leaf HUD key");
        }
    }

    #[test]
    fn a_whole_section_category_never_means_default() {
        // The destructive case: absent_means_default makes apply_to_tree DELETE
        // the target's value. A source with no overview must never wipe one.
        for cat in [
            Category::Layout,
            Category::Autofill,
            Category::Overview,
            Category::OverviewWidths,
            Category::Keybinds,
            Category::ProbeFormations,
            Category::NeocomButtons,
        ] {
            assert!(!cat.absent_means_default(), "{cat:?} must never delete on the target");
        }
    }

    /// A user doc holding the account-side HUD keys the copy cares about.
    fn user_with_hud() -> Value {
        Value::Dict(vec![
            (b("ui"), Value::Dict(vec![(b("shipuialigntop"), Value::Bool(true))])),
            (b("windows"), Value::Dict(vec![(b("neocomWidth"), Value::Int(72))])),
        ])
    }

    #[test]
    fn an_absent_leaf_hud_key_removes_the_targets_own_value() {
        // The source is at EVE's default (no key at all), so the target must end
        // up at the same default rather than keeping its own 72.
        let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        let extracted = extract_categories(&source, &[Category::HudNeocomWidth]);
        assert_eq!(extracted.len(), 1, "the absence is reported, not dropped");
        assert!(extracted[0].1.is_none(), "absence is a removal");

        let mut target = user_with_hud();
        apply_to_tree(&mut target, &extracted);

        let Value::Dict(root) = &target else { panic!("root is a dict") };
        let (_, windows) = root.iter().find(|(k, _)| is_bytes(k, b"windows")).expect("windows survives");
        let Value::Dict(w) = windows else { panic!("windows is a dict") };
        assert!(
            !w.iter().any(|(k, _)| is_bytes(k, b"neocomWidth")),
            "the target's own neocomWidth is gone"
        );
    }

    #[test]
    fn an_absent_whole_section_category_leaves_the_target_alone() {
        // The destructive case. A source with no overview must not wipe one.
        let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        let extracted = extract_categories(&source, &[Category::Overview]);
        assert!(extracted.is_empty(), "a missing section is nothing to copy");

        let mut target = Value::Dict(vec![(b("overview"), Value::Int(7))]);
        apply_to_tree(&mut target, &extracted);
        let Value::Dict(root) = &target else { panic!("root is a dict") };
        assert!(root.iter().any(|(k, _)| is_bytes(k, b"overview")), "the target's overview survives");
    }

    #[test]
    fn a_removal_with_no_parent_section_on_the_target_is_a_no_op() {
        // The source must carry a `notifications` root key, or the badge
        // absence is not a removal at all (see `absence_means_eve_default`) and
        // this would pass without exercising the removal path.
        let source =
            Value::Dict(vec![(b("ui"), Value::Dict(vec![])), (b("notifications"), Value::Dict(vec![]))]);
        let extracted = extract_categories(&source, &[Category::HudBadge]);
        assert_eq!(extracted.len(), 1, "the source really does claim the default");
        let mut target = Value::Dict(vec![(b("keep"), Value::Int(1))]);
        apply_to_tree(&mut target, &extracted);
        let Value::Dict(root) = &target else { panic!("root is a dict") };
        assert!(root.iter().any(|(k, _)| is_bytes(k, b"keep")), "nothing else was touched");
    }

    #[test]
    fn an_empty_root_source_contributes_neither_values_nor_removals() {
        // A Layout preset created before the aspect grew an account side has a
        // user.dat of `{}`. Applying it must not delete the target's HUD keys.
        let extracted = extract_categories(
            &Value::Dict(vec![]),
            &[Category::HudNeocomWidth, Category::HudShipTop],
        );
        assert!(extracted.is_empty(), "a pruned-away side carries no absences");

        let mut target = user_with_hud();
        apply_to_tree(&mut target, &extracted);
        let Value::Dict(root) = &target else { panic!("root is a dict") };
        let (_, windows) = root.iter().find(|(k, _)| is_bytes(k, b"windows")).expect("windows survives");
        let Value::Dict(w) = windows else { panic!("windows is a dict") };
        assert!(
            w.iter().any(|(k, _)| is_bytes(k, b"neocomWidth")),
            "an old preset leaves the target's neocom width alone"
        );
    }

    /// The char.dat a Layout preset saved BEFORE the aspect carried the HUD
    /// has: `windows` and `ui -> neocomButtonRawData`, and no `notifications`.
    fn old_layout_preset_char_doc() -> Value {
        Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("openWindows"), Value::Dict(vec![]))])),
            (
                b("ui"),
                Value::Dict(vec![(
                    b("neocomButtonRawData"),
                    Value::Tuple(vec![ts(), Value::List(vec![b("SOURCE-BAR")])]),
                )]),
            ),
        ])
    }

    /// A character document with the two char-side HUD keys stored.
    fn char_with_hud(fighter: i64, badge: i64) -> Value {
        let point = |v: i64| Value::Tuple(vec![ts(), Value::Tuple(vec![Value::Int(v), Value::Int(v)])]);
        Value::Dict(vec![
            (b("windows"), Value::Dict(vec![])),
            (b("ui"), Value::Dict(vec![(b("fightersDetachedPosition"), point(fighter))])),
            (b("notifications"), Value::Dict(vec![(b("notification_badge_offset"), point(badge))])),
        ])
    }

    #[test]
    fn an_old_layout_presets_char_side_claims_no_hud_absences() {
        // The data-loss case. An old preset's char.dat is NOT an empty root, so
        // the account-side guard never covered it: its missing
        // fightersDetachedPosition and notification_badge_offset were read as
        // "the source is at EVE's default" and DELETED from the target.
        let extracted = extract_categories(
            &old_layout_preset_char_doc(),
            &[Category::Layout, Category::NeocomButtons, Category::HudFighterPos, Category::HudBadge],
        );
        let cats: Vec<Category> = extracted.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            cats,
            vec![Category::Layout, Category::NeocomButtons],
            "an old preset carries what it captured, and claims nothing about the HUD"
        );

        let mut target = char_with_hud(10, 20);
        apply_to_tree(&mut target, &extracted);
        let hud = crate::hud::project_hud(&target, None);
        let val = |n: &str| hud.entries.iter().find(|e| e.name == n).unwrap().value.clone();
        assert_eq!(val("fighter_x").as_deref(), Some("10"), "the target's fighter panel survives");
        assert_eq!(val("badge_x").as_deref(), Some("20"), "the target's badge offset survives");
    }

    #[test]
    fn a_char_source_with_the_notifications_section_still_removes() {
        // The other half of the discriminator: a document shaped like a real
        // character file (or a Layout preset created after this branch) keeps
        // full removal semantics, so a copy from a character sitting at EVE's
        // defaults still resets the target.
        let source = Value::Dict(vec![
            (b("ui"), Value::Dict(vec![])),
            (b("notifications"), Value::Dict(vec![])),
        ]);
        let extracted =
            extract_categories(&source, &[Category::HudFighterPos, Category::HudBadge]);
        assert_eq!(extracted.len(), 2, "both absences are claimed as EVE's default");
        assert!(extracted.iter().all(|(_, v)| v.is_none()), "both are removals");

        let mut target = char_with_hud(10, 20);
        apply_to_tree(&mut target, &extracted);
        let hud = crate::hud::project_hud(&target, None);
        for name in ["fighter_x", "fighter_y", "badge_x", "badge_y"] {
            let e = hud.entries.iter().find(|e| e.name == name).unwrap();
            assert!(e.value.is_none(), "{name} fell back to EVE's default");
        }
    }

    #[test]
    fn a_present_leaf_hud_key_is_copied_over_the_targets_own() {
        let extracted = extract_categories(&user_with_hud(), &[Category::HudNeocomWidth]);
        let mut target = Value::Dict(vec![(
            b("windows"),
            Value::Dict(vec![(b("neocomWidth"), Value::Int(37))]),
        )]);
        apply_to_tree(&mut target, &extracted);
        let Value::Dict(root) = &target else { panic!("root is a dict") };
        let (_, windows) = root.iter().find(|(k, _)| is_bytes(k, b"windows")).expect("windows exists");
        let Value::Dict(w) = windows else { panic!("windows is a dict") };
        let (_, v) = w.iter().find(|(k, _)| is_bytes(k, b"neocomWidth")).expect("the key was copied");
        assert_eq!(*v, Value::Int(72), "the source's width won");
    }
}
