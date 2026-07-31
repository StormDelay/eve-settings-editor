//! Regenerates the synthetic corpus in `fixtures/synthetic/`.
//!
//!     cargo run -p settings-model --bin gen_fixtures
//!
//! Why this exists: the real corpus (`testdata/`) is personal data and is
//! gitignored, so every corpus gate silently skips on CI and on any other
//! machine. The synthetic corpus is a committed, deterministic stand-in that is
//! small (tens of KB, not 574 MB) and — unlike the real corpus, where 6140 files
//! collapse to 413 distinct — carries no duplication at all: every file is here
//! because it covers a shape nothing else covers.
//!
//! What the fixtures are and are not:
//!
//! - `codec/` and `profile/` are **golden files**: a `Value` tree built below,
//!   run through `blue_marshal::encode`, and committed. The byte-identity gate
//!   over them is therefore a *regression* check (encoder output must not drift)
//!   and a decoder-coverage check, NOT an independent proof that our bytes match
//!   CCP's. The real corpus remains the only independence proof, and it still
//!   runs locally.
//! - `decode-only/` is hand-authored wire bytes for opcodes the encoder never
//!   emits (deprecated STRING/STRINGL). It is excluded from the byte-identity
//!   gate by directory name.
//!
//! Every id below is synthetic. No value here came from a real file.

use std::path::{Path, PathBuf};

use blue_marshal::{decode, encode, reshare, Value};

// ---------------------------------------------------------------- helpers

fn b(s: &str) -> Value {
    Value::Bytes(s.as_bytes().to_vec())
}
fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}
fn i(n: i64) -> Value {
    Value::Int(n)
}
fn f(x: f64) -> Value {
    Value::Float(x)
}
fn list(items: Vec<Value>) -> Value {
    Value::List(items)
}
fn tup(items: Vec<Value>) -> Value {
    Value::Tuple(items)
}
fn dict(entries: Vec<(Value, Value)>) -> Value {
    Value::Dict(entries)
}

/// A Windows FILETIME as the client stores it: 8-byte little-endian LONG.
fn filetime(n: i64) -> Value {
    Value::Long(n.to_le_bytes().to_vec())
}

/// The `(FILETIME, value)` leaf wrapper almost every setting uses.
fn w(v: Value) -> Value {
    tup(vec![filetime(134_291_947_598_915_031), v])
}

/// Same wrapper with a distinct timestamp, so a file contains more than one and
/// `reshare` has something to actually dedupe.
fn w2(v: Value) -> Value {
    tup(vec![filetime(134_291_957_312_161_688), v])
}

/// A 6-tuple window rect: (x, y, w, h, screenW, screenH).
fn rect(x: i64, y: i64, wd: i64, h: i64, sw: i64, sh: i64) -> Value {
    tup(vec![i(x), i(y), i(wd), i(h), i(sw), i(sh)])
}

fn flags(entries: &[(&str, bool)]) -> Value {
    w(dict(entries.iter().map(|(k, v)| (b(k), Value::Bool(*v))).collect()))
}

// ------------------------------------------------------- codec fixtures

/// Every scalar opcode the decoder supports and the encoder can re-emit,
/// including the three the real corpus provably never contains: a **negative**
/// LONG, a short (1-byte) LONG, and a STRINGR table reference sitting next to a
/// UTF8 string with the same content (the collision that forces the `StrTable`
/// fidelity tag).
fn codec_scalars() -> Value {
    dict(vec![
        (b("none"), Value::None),
        (b("true"), Value::Bool(true)),
        (b("false"), Value::Bool(false)),
        (b("minusone"), i(-1)),
        (b("zero"), i(0)),
        (b("one"), i(1)),
        (b("int8"), i(-128)),
        (b("int16"), i(-32768)),
        (b("int32"), i(-2_147_483_648)),
        (b("int64"), i(i64::MIN)),
        (b("float"), f(0.7)),
        (b("float_neg_zero"), f(-0.0)),
        (b("float0"), f(0.0)),
        (b("long_filetime"), filetime(134_291_947_598_915_031)),
        (b("long_negative"), Value::Long((-42i64).to_le_bytes().to_vec())),
        (b("long_short"), Value::Long(vec![0x7f])),
        (b("long_zero_bytes"), Value::Long(vec![])),
        (b("bytes_empty"), Value::Bytes(vec![])),
        (b("bytes_one"), Value::Bytes(vec![0x41])),
        (b("bytes_buffer"), b("a longer byte string")),
        (b("bytes_high"), Value::Bytes(vec![0x00, 0xff, 0x7e, 0x80])),
        (b("utf8_empty"), s("")),
        (b("utf8_text"), s("ordinary text")),
        (b("utf8_non_ascii"), s("\u{e9}\u{fc}\u{4e2d}\u{6587}")),
        (b("ucs2_empty"), Value::StrUcs2(String::new())),
        (b("ucs2_one"), Value::StrUcs2("x".to_string())),
        (b("ucs2_many"), Value::StrUcs2("wide string".to_string())),
        // STRINGR index 52 is "name"; the UTF8 sibling holds the same text, the
        // 1269-occurrence collision that makes `StrTable` un-inferable.
        (b("strtable"), Value::StrTable(52)),
        (b("strtable_collision"), s("name")),
    ])
}

/// Container shapes, including the empty and 1/2-element variants that have
/// their own opcodes, deep nesting, and the 0xFF length escape on all three of
/// list, dict and byte payload.
fn codec_containers() -> Value {
    let long_list: Vec<Value> = (0..300).map(i).collect();
    let big_dict: Vec<(Value, Value)> =
        (0..300).map(|n| (Value::Int(n), Value::Int(n * 2))).collect();
    dict(vec![
        (b("tuple0"), tup(vec![])),
        (b("tuple1"), tup(vec![i(1)])),
        (b("tuple2"), tup(vec![i(1), i(2)])),
        (b("tuple_counted"), tup(vec![i(1), i(2), i(3), i(4)])),
        (b("list0"), list(vec![])),
        (b("list1"), list(vec![i(1)])),
        (b("list_counted"), list(vec![i(1), i(2)])),
        (b("dict_empty"), dict(vec![])),
        (b("nested"), list(vec![tup(vec![dict(vec![(b("deep"), list(vec![i(1)]))])])])),
        // 0xFF length escape: > 255 elements / entries / bytes.
        (b("list_len_escape"), list(long_list)),
        (b("dict_len_escape"), dict(big_dict)),
        (b("bytes_len_escape"), Value::Bytes(vec![0x61; 400])),
        (b("utf8_len_escape"), s(&"y".repeat(400))),
    ])
}

/// Sharing mechanics, authored explicitly rather than via `reshare` so the tail
/// map is deliberately **non-identity** (slots 3, 1, 2 in encounter order) — the
/// dominant real-corpus shape (963,660 out-of-order entries) and the one an
/// encoder that assumed identity would break on.
///
/// Also covers the three indirection traps documented in the field reference:
/// a `Ref` as a dict KEY, a `Ref` inside a TUPLE key, and a `Ref` in the
/// timestamp slot of a `(FILETIME, value)` wrapper.
fn codec_sharing() -> Value {
    dict(vec![
        // Definitions first: the encoder requires store-before-ref.
        (b("def_bytes"), Value::Shared { slot: 3, value: Box::new(b("overviewScroll2")) }),
        (b("def_time"), Value::Shared { slot: 1, value: Box::new(filetime(134_291_947_598_915_031)) }),
        (b("def_global"), Value::Shared { slot: 2, value: Box::new(Value::Global(b"__builtin__.set".to_vec())) }),
        // A Ref standing in for the timestamp half of the wrapper.
        (b("ts_is_a_ref"), tup(vec![Value::Ref(1), i(7)])),
        // A Ref used directly as a dict key.
        (b("ref_keyed"), dict(vec![(Value::Ref(3), i(1))])),
        // A Ref nested inside a tuple key — the real on-disk form of the
        // overview column-width key.
        (b("tuple_key_with_ref"), dict(vec![(tup(vec![Value::Ref(3), i(0)]), i(63))])),
        // The same Global reached through a Ref.
        (b("ref_global"), Value::Ref(2)),
        // Same byte content as slot 3 but NOT shared: proves the decoder keeps
        // `Shared`/`Ref` explicit instead of inferring it from content.
        (b("unshared_twin"), b("overviewScroll2")),
    ])
}

/// GLOBAL / INSTANCE / REDUCE / STREAM. The non-empty REDUCE iterator tail and
/// the STREAM opcode are both shapes the real corpus contains **zero** of, so
/// this is their only coverage anywhere.
fn codec_objects() -> Value {
    dict(vec![
        (b("global"), Value::Global(b"__builtin__.set".to_vec())),
        (
            b("instance"),
            Value::Instance {
                class: Box::new(b("utillib.KeyVal")),
                state: Box::new(dict(vec![(b("id"), b("agency")), (b("btnType"), i(1))])),
            },
        ),
        // The empty-tail form: every REDUCE in the real corpus looks like this.
        (
            b("reduce_empty_tail"),
            Value::Reduce {
                ctor: Box::new(tup(vec![
                    Value::Global(b"__builtin__.set".to_vec()),
                    tup(vec![list(vec![i(11), i(22)])]),
                ])),
                items: vec![],
                pairs: vec![],
            },
        ),
        // The general MARK-delimited form the decoder implements but no real
        // file exercises.
        (
            b("reduce_full_tail"),
            Value::Reduce {
                ctor: Box::new(tup(vec![
                    Value::Global(b"__builtin__.set".to_vec()),
                    tup(vec![list(vec![])]),
                ])),
                items: vec![i(1), b("item")],
                pairs: vec![(b("k1"), i(1)), (b("k2"), b("v2"))],
            },
        ),
        // STREAM: an embedded marshal blob with its own slot scope.
        (
            b("stream"),
            Value::Stream(Box::new(dict(vec![(b("inner"), list(vec![i(1), b("nested")]))]))),
        ),
    ])
}

/// Dict keys of every kind the corpus contains, including the two anomalies the
/// account file really carries: a literal `None` key, and a tuple key mixing
/// bytes, a nested tuple and a float.
fn codec_odd_keys() -> Value {
    dict(vec![
        (b("bytes key"), i(1)),
        (s("str key"), i(2)),
        (Value::StrUcs2("ucs2 key".into()), i(3)),
        (i(42), i(4)),
        (Value::Bool(true), i(5)),
        (Value::Bool(false), i(6)),
        (Value::None, i(7)),
        (Value::StrTable(29), i(8)),
        (tup(vec![b("background"), i(13)]), tup(vec![f(0.0), f(0.15), f(0.6), f(1.0)])),
        (tup(vec![b("windowTransparency"), tup(vec![b("user"), b("ui")]), f(0.5)]), f(0.0)),
        // A Bytes payload that merely starts with the marshal magic byte. It is
        // NOT a nested stream and must stay opaque (8739 such payloads in the
        // real corpus, zero of them decodable).
        (b("magic_prefixed_blob"), Value::Bytes(vec![0x7e, 0x01, 0x02, 0x03])),
    ])
}

// ----------------------------------------------------- profile fixtures

const OVERVIEW_COLUMNS: [&str; 14] = [
    "ICON", "DISTANCE", "NAME", "TYPE", "CORPORATION", "ALLIANCE", "FACTION", "MILITIA", "SIZE",
    "VELOCITY", "RADIALVELOCITY", "TRANSVERSALVELOCITY", "ANGULARVELOCITY", "TAG",
];

fn col_list(names: &[&str]) -> Value {
    list(names.iter().map(|n| b(n)).collect())
}

/// A modern character on a 2560x1440 client: every window flag dict, three
/// overview windows, a window stack (numeric container + two members + tab
/// order), all three window-id flavours (plain, numeric, stringified tuple),
/// both HUD anchors in their real sections, per-tab column widths and sort, and
/// the five always-empty sections.
fn char_modern() -> Value {
    let win_ids = ["overview", "overview_1", "overview_2", "market", "fitting", "chatchannel_local"];
    let mut geometry = vec![
        (b("overview"), rect(2114, 424, 446, 1016, 2560, 1440)),
        (b("overview_1"), rect(1707, 288, 853, 400, 2560, 1440)),
        (b("overview_2"), rect(1707, 700, 853, 400, 2560, 1440)),
        (b("market"), rect(16, 825, 1004, 800, 2560, 1440)),
        (b("fitting"), rect(300, 100, 600, 500, 2560, 1440)),
        (b("chatchannel_local"), rect(0, 918, 256, 522, 2560, 1440)),
        // The stack: container and both members share one identical rect.
        (b("7001"), rect(100, 100, 400, 300, 2560, 1440)),
        (b("addressbook"), rect(100, 100, 400, 300, 2560, 1440)),
        (b("calendar"), rect(100, 100, 400, 300, 2560, 1440)),
        // Stringified-tuple window ids, both flavours seen in real files.
        (s("('corpassets', 1000000000001L)"), rect(500, 200, 700, 600, 2560, 1440)),
        (s("('myPlaces', (9000001, None))"), rect(600, 300, 400, 400, 2560, 1440)),
    ];
    geometry.sort_by_key(|(k, _)| format!("{k:?}"));

    let mut open: Vec<(Value, Value)> = win_ids.iter().map(|n| (b(n), Value::Bool(true))).collect();
    open.extend([
        (b("7001"), Value::Bool(true)),
        (b("addressbook"), Value::Bool(true)),
        (b("calendar"), Value::Bool(true)),
        (s("('corpassets', 1000000000001L)"), Value::Bool(true)),
        (s("('myPlaces', (9000001, None))"), Value::Bool(false)),
    ]);

    dict(vec![
        (
            b("windows"),
            dict(vec![
                (b("__version__"), w(i(1))),
                (b("__usercopy__"), w(Value::Bool(true))),
                (b("windowSizesAndPositions_1"), w(dict(geometry))),
                (b("openWindows"), w(dict(open))),
                (
                    b("minimizedWindows"),
                    flags(&[("overview", false), ("market", false), ("7001", false)]),
                ),
                (b("collapsedWindows"), flags(&[("market", false)])),
                (b("compactWindows"), flags(&[("overview", true), ("overview_1", true)])),
                (b("lockedWindows"), flags(&[("overview", true)])),
                (b("pinnedWindows"), flags(&[("overview", true), ("overview_1", false)])),
                (
                    b("isOverlayedWindows"),
                    flags(&[("overview", false), ("7001", false), ("addressbook", false)]),
                ),
                (
                    b("isLightBackgroundWindows"),
                    flags(&[("overview", false), ("7001", false), ("calendar", false)]),
                ),
                // Stack membership: member -> container id, plus an explicitly
                // unstacked window (None, never Int — the Int branch in
                // windows.rs is dead on real data).
                (
                    b("stacksWindows"),
                    w(dict(vec![
                        (b("addressbook"), b("7001")),
                        (b("calendar"), b("7001")),
                        (b("market"), Value::None),
                    ])),
                ),
                (
                    b("preferredIdxInStack3"),
                    w(dict(vec![(
                        b("7001"),
                        dict(vec![(b("addressbook"), i(0)), (b("calendar"), i(1))]),
                    )])),
                ),
                (b("shipuialignleftoffset"), w(f(-189.0))),
                (b("wndColorThemeID"), w(b("UI/ColorThemes/Photon"))),
                (b("baseColorTemp"), w(Value::None)),
                (b("hiliteColorTemp"), w(Value::None)),
            ]),
        ),
        (
            b("notifications"),
            dict(vec![
                // Real section for the badge anchor — `ui` has never held it.
                (b("notification_badge_offset"), w(tup(vec![i(2519), i(131)]))),
                (b("notificationSettingsRepositionCount"), w(i(2))),
                (b("lastSeenNotificationId"), w(i(1084915628))),
                (b("lastSeenNotificationTime"), w(filetime(134_291_947_444_598_954))),
            ]),
        ),
        (
            b("ui"),
            dict(vec![
                (b("fightersDetachedPosition"), w(tup(vec![i(326), i(54)]))),
                // Per-tab overview column widths, keyed by a tuple whose first
                // element repeats across every entry (so `reshare` interns it
                // and the on-disk key becomes `(Ref, Int)`).
                (
                    b("SortHeadersSizes"),
                    w(dict(vec![
                        (
                            tup(vec![b("overviewScroll2"), i(0)]),
                            dict(vec![
                                (b("ICON"), i(24)),
                                (b("DISTANCE"), i(63)),
                                (b("NAME"), i(120)),
                                (b("TYPE"), i(80)),
                            ]),
                        ),
                        (
                            tup(vec![b("overviewScroll2"), i(1)]),
                            dict(vec![(b("ICON"), i(24)), (b("ALLIANCE"), i(46))]),
                        ),
                    ])),
                ),
                (
                    b("SortHeadersSettings2"),
                    w(dict(vec![
                        (tup(vec![b("overviewScroll2"), i(0)]), tup(vec![b("DISTANCE"), Value::Bool(true)])),
                        (tup(vec![b("overviewScroll2"), i(1)]), tup(vec![b("NAME"), Value::Bool(false)])),
                    ])),
                ),
                (b("windowTransparency"), w(f(0.0))),
                (b("neocomSizeLocked"), w(Value::Bool(true))),
                (b("spaceCameraID"), w(b("shiporbit"))),
                (b("pfRouteType"), w(b("safe"))),
                (b("autopilot_waypoints"), w(list(vec![i(30000142)]))),
                (b("chatchannels"), w(list(vec![tup(vec![b("local"), s("local_30000142"), s("Local")])]))),
                // The neocom button bar: two `utillib.KeyVal` instances, corpus
                // key order (btnType, children, iconPath, id) — one plain button
                // and one folder with its single Inventory child (format-notes.md
                // "Neocom buttons").
                (
                    b("neocomButtonRawData"),
                    w(list(vec![
                        Value::Instance {
                            class: Box::new(b("utillib.KeyVal")),
                            state: Box::new(dict(vec![
                                (b("btnType"), i(10)),
                                (b("children"), Value::None),
                                (b("iconPath"), b("res:/ui/Texture/WindowIcons/chatchannel.png")),
                                (b("id"), b("chat")),
                            ])),
                        },
                        Value::Instance {
                            class: Box::new(b("utillib.KeyVal")),
                            state: Box::new(dict(vec![
                                (b("btnType"), i(4)),
                                (
                                    b("children"),
                                    list(vec![Value::Instance {
                                        class: Box::new(b("utillib.KeyVal")),
                                        state: Box::new(dict(vec![
                                            (b("btnType"), i(4)),
                                            (b("children"), Value::None),
                                            (b("iconPath"), b("res:/UI/Texture/WindowIcons/station.png")),
                                            (b("id"), b("InventoryStation")),
                                        ])),
                                    }]),
                                ),
                                (b("iconPath"), b("res:/UI/Texture/WindowIcons/items.png")),
                                (b("id"), b("inventory")),
                            ])),
                        },
                    ])),
                ),
            ]),
        ),
        (
            b("dockPanels"),
            dict(vec![(
                b("primary_map_panel"),
                dict(vec![
                    (b("align"), i(0)),
                    (b("dblToggleFullScreenAlign"), i(0)),
                    (b("heightProportion"), f(0.8)),
                    (b("heightProportion_docked"), f(1.0)),
                    (b("positionX"), f(0.5)),
                    (b("positionY"), f(0.5)),
                    (b("pushedBy"), list(vec![])),
                    (b("widthProportion"), f(0.8)),
                    (b("widthProportion_docked"), f(0.5)),
                ]),
            )]),
        ),
        (b("notepad"), dict(vec![(b("activeNote"), w(b("N:90000001")))])),
        // Keyed by item id, values as the client writes them.
        (b("autorepeat"), dict(vec![(filetime(1_030_000_000_001), i(1000))])),
        (b("autoreload"), dict(vec![(filetime(1_030_000_000_001), i(1))])),
        // The five sections that are always present and always empty.
        (b("generic"), dict(vec![])),
        (b("inbox"), dict(vec![])),
        (b("zaction"), dict(vec![])),
        (b("shiptheme"), dict(vec![])),
        (b("enableWindowBlur"), dict(vec![])),
    ])
}

/// The absent-means-default character: geometry and open flags only. No HUD
/// keys, no stacks, no column widths, no colour theme — every optional read path
/// must return "not set" rather than panicking or inventing a zero.
fn char_minimal() -> Value {
    dict(vec![
        (
            b("windows"),
            dict(vec![
                (b("__version__"), w(i(1))),
                (
                    b("windowSizesAndPositions_1"),
                    w(dict(vec![(b("overview"), rect(0, 0, 400, 900, 1920, 1080))])),
                ),
                (b("openWindows"), w(dict(vec![(b("overview"), Value::Bool(true))]))),
                (b("minimizedWindows"), w(dict(vec![]))),
            ]),
        ),
        (b("ui"), dict(vec![])),
        // Present but empty. Every one of the 384 corpus character files carries
        // a `notifications` section; only the badge KEY inside it is optional
        // (313/384). Omitting the section entirely would be less realistic than
        // any real file, and would test a mint path that cannot occur.
        (b("notifications"), dict(vec![])),
        (b("generic"), dict(vec![])),
    ])
}

/// An older character: 1920x1080, `Int` where the modern client writes `Float`
/// or `Bool`, no `pinnedWindows`, and a legacy migration flag. Guards the
/// type-instability trap — a reader that hard-matches one variant reads nothing
/// here.
fn char_legacy() -> Value {
    dict(vec![
        (
            b("windows"),
            dict(vec![
                (b("__version__"), w(i(1))),
                (b("__clear_stored_compact_window_settings_that_match_the_default__"), w(Value::Bool(true))),
                (
                    b("windowSizesAndPositions_1"),
                    w(dict(vec![
                        (b("overview"), rect(1544, 55, 446, 1016, 1920, 1080)),
                        (b("market"), rect(100, 100, 800, 600, 1920, 1080)),
                    ])),
                ),
                (
                    b("openWindows"),
                    w(dict(vec![(b("overview"), Value::Bool(true)), (b("market"), Value::Bool(false))])),
                ),
                (b("minimizedWindows"), w(dict(vec![(b("overview"), Value::Bool(false))]))),
                // Int, not Float — two real files do exactly this.
                (b("shipuialignleftoffset"), w(i(0))),
            ]),
        ),
        (
            b("notifications"),
            dict(vec![
                (b("notification_badge_offset"), w(tup(vec![i(36), i(46)]))),
                // Long, not Int — the other half of the instability.
                (b("lastSeenNotificationId"), w(filetime(925_532_746))),
            ]),
        ),
        (b("ui"), dict(vec![(b("windowTransparency"), w(f(0.0)))])),
        (b("generic"), dict(vec![])),
    ])
}

fn preset(groups: &[i64], filtered: &[i64], always: &[i64]) -> Value {
    dict(vec![
        (b("groups"), list(groups.iter().map(|n| i(*n)).collect())),
        (b("filteredStates"), list(filtered.iter().map(|n| i(*n)).collect())),
        (b("alwaysShownStates"), list(always.iter().map(|n| i(*n)).collect())),
    ])
}

fn tab(name: &str, overview: &str, bracket: &str, cols: &[&str]) -> Value {
    dict(vec![
        (Value::StrTable(52), b(name)),
        (b("overview"), b(overview)),
        (b("bracket"), b(bracket)),
        (b("color"), Value::None),
        (b("tabColumnOrder"), col_list(&OVERVIEW_COLUMNS)),
        (b("tabColumns"), col_list(cols)),
    ])
}

/// A modern account carrying every account-scoped domain the field reference
/// found: the full overview container (all three tabsettings generations, the
/// 6+1+1 window mapping, presets, both state-colour surfaces, blinks, ship
/// labels, all 13 booleans), keybinds, audio, suppress, tabgroups, and a `ui`
/// section with the graphics cluster, autofill history and both key anomalies.
fn user_modern() -> Value {
    let tabs = vec![
        (i(0), tab("  *  ", "basic travel", "no brackets", &["ICON", "DISTANCE", "NAME", "TYPE"])),
        (i(1), tab("  2  ", "hostile", "default brackets", &["ICON", "DISTANCE", "NAME", "ALLIANCE"])),
        (i(2), tab("  3  ", "friendly", "default brackets", &["ICON", "NAME", "TYPE"])),
        (i(3), tab("  4  ", "structures", "no brackets", &["ICON", "DISTANCE", "NAME"])),
        (i(4), tab("  5  ", "basic travel", "no brackets", &["ICON", "NAME"])),
        (i(5), tab("  6  ", "hostile", "default brackets", &["ICON", "NAME"])),
        (i(6), tab("  7  ", "friendly", "default brackets", &["ICON", "NAME"])),
        (i(7), tab("  8  ", "basic travel", "no brackets", &["ICON", "NAME"])),
    ];
    // The lopsided 6+1+1 split 812 of 823 real mappings use.
    let by_window = list(vec![
        list(vec![i(0), i(1), i(2), i(3), i(4), i(7)]),
        list(vec![i(5)]),
        list(vec![i(6)]),
    ]);

    let ship_labels = list(vec![
        dict(vec![
            (b("pre"), b("<fontsize=11><color=0xFFF5DEB3><b>")),
            (b("post"), b("</b></color></fontsize>  [")),
            (b("state"), i(1)),
            (b("type"), b("ship type")),
        ]),
        dict(vec![
            (b("pre"), b("<color=0xFFFFB900>")),
            (b("post"), b("</color>")),
            (b("state"), i(1)),
            (b("type"), b("alliance")),
        ]),
        dict(vec![
            (b("pre"), b("[")),
            (b("post"), b("")),
            (b("state"), i(0)),
            (b("type"), Value::None),
        ]),
    ]);

    let overview = dict(vec![
        (
            b("overviewProfilePresets"),
            w(dict(vec![
                (b("basic travel"), preset(&[1, 5, 11], &[], &[])),
                (b("hostile"), preset(&[6, 7, 11], &[36], &[13])),
                (b("friendly"), preset(&[11, 18], &[], &[])),
                (b("structures"), preset(&[65, 66], &[37], &[])),
            ])),
        ),
        (
            b("overviewProfilePresets_notSaved"),
            w(dict(vec![(b("hostile"), preset(&[6, 7], &[], &[]))])),
        ),
        (b("activeOverviewPreset"), w(b("hostile"))),
        (b("tabsettings_new"), w(dict(tabs.clone()))),
        // Legacy mirror: same tab indices, no column keys — the client
        // timestamp-bumps it without changing its content.
        (
            b("tabsettings"),
            w2(dict(
                tabs.iter()
                    .map(|(k, _)| {
                        (
                            k.clone(),
                            dict(vec![
                                (Value::StrTable(52), b("  legacy  ")),
                                (b("overview"), b("basic travel")),
                                (b("bracket"), b("no brackets")),
                            ]),
                        )
                    })
                    .collect(),
            )),
        ),
        // Abandoned generation: present, stale, never rewritten by the client.
        (
            b("tabsettings2"),
            w2(dict(vec![(i(0), tab("  stale  ", "friendly", "no brackets", &["ICON"]))])),
        ),
        (b("tabsByWindowInstanceID"), w(by_window)),
        (b("overviewColumns"), w(col_list(&["ICON", "DISTANCE", "NAME", "TYPE"]))),
        (b("overviewColumnOrder"), w(col_list(&OVERVIEW_COLUMNS))),
        (b("backgroundStates2"), w(list(vec![i(9), i(10), i(11), i(12), i(13), i(14)]))),
        (b("backgroundOrder2"), w(list(vec![i(13), i(44), i(52), i(11), i(12), i(14), i(68)]))),
        (b("flagStates2"), w(list(vec![i(9), i(13), i(44)]))),
        (b("flagOrder2"), w(list(vec![i(13), i(44), i(52), i(9), i(68)]))),
        (
            b("stateColors"),
            w(dict(vec![
                (tup(vec![b("background"), i(13)]), tup(vec![f(0.0), f(0.15), f(0.6), f(1.0)])),
                (tup(vec![b("background"), i(44)]), tup(vec![f(0.6), f(0.0), f(0.0), f(1.0)])),
                // The second surface: rare (2/175 real files) but real.
                (tup(vec![Value::StrTable(29), i(48)]), tup(vec![f(1.0), f(1.0), f(0.0), f(1.0)])),
            ])),
        ),
        (
            b("stateBlinks"),
            w(dict(vec![
                (tup(vec![b("background"), i(13)]), Value::Bool(false)),
                (tup(vec![Value::StrTable(29), i(13)]), Value::Bool(true)),
            ])),
        ),
        (b("shipLabels"), w(ship_labels)),
        // All 13 appearance booleans, including the 7 no editor exposes.
        (b("applyToStructures"), w(Value::Bool(true))),
        (b("applyToOtherObjects"), w(Value::Bool(false))),
        (b("useSmallColorTags"), w(Value::Bool(false))),
        (b("useSmallText"), w(Value::Bool(false))),
        (b("overviewBroadcastsToTop"), w(Value::Bool(true))),
        (b("hideCorpTicker"), w(Value::Bool(false))),
        (b("targetCrosshair"), w(Value::Bool(true))),
        (b("showInTargetRange"), w(Value::Bool(true))),
        (b("showCategoryInTargetRange_6"), w(Value::Bool(true))),
        (b("showCategoryInTargetRange_11"), w(Value::Bool(true))),
        (b("showCategoryInTargetRange_18"), w(Value::Bool(true))),
        (b("showBiggestDamageDealers"), w(Value::Bool(true))),
        (b("showModuleHairlines"), w(Value::Bool(true))),
        (b("viewTactical"), w(Value::Bool(false))),
        (b("viewTactical_camTactical"), w(Value::Bool(true))),
    ]);

    let keybinds = dict(vec![
        (b("CmdActivateHighPowerSlot1"), tup(vec![i(81)])),
        (b("CmdActivateHighPowerSlot2"), tup(vec![i(83)])),
        (b("CmdActivateMediumPowerSlot1"), tup(vec![i(17), i(81)])),
        (b("CmdDronesReturnAndOrbit"), tup(vec![i(18), i(16), i(68)])),
        (b("CmdActivateLowPowerSlot1"), Value::None),
        (b("OpenFitting"), tup(vec![i(65)])),
    ]);

    let audio = dict(vec![
        (b("soundLevel_advancedSettings"), w(i(1))),
        (b("inactiveSounds_music"), w(i(1))),
        (b("inactiveSounds_aura"), w(i(0))),
        (b("custom_shipsounds"), w(f(0.5))),
        (b("soundLevel_custom_shipsounds"), w(f(0.5))),
    ]);

    let suppress = dict(vec![
        (b("suppress.AskQuitGame"), w(i(6))),
        (b("suppress.ConfirmJumpToUnsafeSS"), w(i(1))),
        (b("suppress.TradeShipWarning"), w(i(1))),
    ]);

    // Stack tab state, keyed by the same container id the character file mints.
    let tabgroups = dict(vec![
        (b("7001"), w(i(1))),
        (b("7001_names"), w(s("Character: Information"))),
    ]);

    let edit_history = w(dict(vec![
        (
            b("/addressbook/content/main/SearchPanel/Container/SingleLineEditText"),
            list(vec![s("first search"), s("second search")]),
        ),
        (b("/market/quickbar/search"), list(vec![s("tritanium"), Value::Bytes(vec![])])),
    ]));

    let ui = dict(vec![
        (b("editHistory"), edit_history),
        // The account-scoped graphics/camera cluster.
        (b("cameraShakeEnabled"), w(i(0))),
        (b("cameraInertia"), w(f(0.0))),
        (b("effectsEnabled"), w(i(1))),
        (b("trailsEnabled"), w(i(1))),
        (b("turretsEnabled"), w(i(1))),
        (b("UI_ASTEROID_FOG"), w(i(1))),
        (b("disabledGuids"), w(dict(vec![(b("effects.AbyssalSpaceTear"), Value::Bool(true))]))),
        // HUD, account half.
        (b("shipuialigntop"), w(Value::Bool(true))),
        (b("detachFighterUI"), w(Value::Bool(true))),
        (b("displayFighterUI"), w(Value::Bool(true))),
        // The locked-target list, in its real section (`ui`, not `windows` —
        // see hud.rs). The pair is a FRACTION: y over the screen height, x over
        // the width right of the neocom, so 0.5442122186495176 = 1354/2488 is
        // a real client's exact rational at 2560 wide with a 72px neocom.
        (b("targetOrigin"), w(tup(vec![f(0.5442122186495176), f(0.5222222222222223)]))),
        (b("targetOriginLocked"), w(i(0))),
        (b("alignHorizontally"), w(Value::Bool(true))),
        // Probe scanner filter naming an overview preset — the cross-container
        // reference `rename_preset` does not currently retarget.
        (b("scanner_presetInUse"), w(b("hostile"))),
        (b("suppress"), w(i(1))),
        (b("windowTransparency"), w(f(0.0))),
        // Type instability: Int where the modern client writes Bool.
        (b("FMBQsearchTitles"), w(i(1))),
        // The two real anomalies: a literal None key and a tuple key.
        (Value::None, w(i(1))),
        (
            tup(vec![b("windowTransparency"), tup(vec![b("user"), b("ui")]), f(0.0)]),
            w(f(0.0)),
        ),
    ]);

    dict(vec![
        (b("overview"), overview),
        (b("ui"), ui),
        (b("cmd"), dict(vec![(b("customCmds"), w(keybinds))])),
        (b("audio"), audio),
        (b("suppress"), suppress),
        (b("tabgroups"), tabgroups),
        (b("windows"), dict(vec![(b("neocomWidth"), w(i(37)))])),
        (
            b("defaultoverview"),
            dict(vec![
                (b("defaultOverviewID"), w(b("synthetic_default"))),
                (b("defaultOverviewInformedOfUpdate"), w(i(1))),
                (b("overviewID"), w(Value::None)),
            ]),
        ),
        (b("localization"), dict(vec![])),
        (b("notifications"), dict(vec![])),
    ])
}

/// A clean account: the overview container exists but is empty, so every
/// overview read must fall back to the client's built-in defaults rather than
/// projecting nothing or erroring. This is the state the default-profile
/// support was built for.
fn user_clean() -> Value {
    dict(vec![
        (b("overview"), dict(vec![])),
        (b("ui"), dict(vec![])),
        (b("cmd"), dict(vec![(b("customCmds"), w(dict(vec![])))])),
        (b("audio"), dict(vec![])),
        (b("suppress"), dict(vec![])),
        (b("tabgroups"), dict(vec![])),
        (b("windows"), dict(vec![])),
        (b("localization"), dict(vec![])),
        (b("notifications"), dict(vec![])),
    ])
}

/// A legacy account: `tabsettings` only, no `tabsettings_new`, and booleans
/// stored as `Int`. Exercises the tab-key migration path and the type-tolerant
/// boolean read.
fn user_legacy() -> Value {
    dict(vec![
        (
            b("overview"),
            dict(vec![
                (
                    b("overviewProfilePresets"),
                    w(dict(vec![(b("legacy preset"), preset(&[1, 5], &[], &[]))])),
                ),
                (
                    b("tabsettings"),
                    w(dict(vec![
                        (i(0), tab("  *  ", "legacy preset", "legacy preset", &["ICON", "NAME"])),
                        (i(1), tab("  2  ", "legacy preset", "legacy preset", &["ICON"])),
                    ])),
                ),
                (b("overviewColumns"), w(col_list(&["ICON", "NAME"]))),
                (b("overviewColumnOrder"), w(col_list(&OVERVIEW_COLUMNS))),
                // Int, not Bool.
                (b("hideCorpTicker"), w(i(0))),
                (b("applyToStructures"), w(i(1))),
            ]),
        ),
        (b("ui"), dict(vec![(b("editHistory"), w(dict(vec![])))])),
        (b("localization"), dict(vec![])),
    ])
}

/// The interning shape `reshare` deliberately never produces but real files use
/// constantly: the state lists reached through a `Ref` to a `Shared` **List**
/// defined at an unrelated sibling key, and a preset's exception lists as bare
/// `Ref`s with no `(FILETIME, _)` wrapper at all. A projection that matches
/// `Value::List` directly reads nothing here.
///
/// Authored explicitly, so slots are numbered in document order (dense, and
/// every definition precedes its reference — the encoder requires both) and
/// this file is NOT passed through `reshare`.
fn user_interned() -> Value {
    dict(vec![(
        b("overview"),
        dict(vec![
            // Definitions parked at keys no lookup matches by name.
            (i(900), Value::Shared { slot: 1, value: Box::new(list(vec![i(9), i(13)])) }),
            (i(901), Value::Shared { slot: 2, value: Box::new(list(vec![i(13), i(9), i(68)])) }),
            (i(902), Value::Shared { slot: 3, value: Box::new(list(vec![i(36)])) }),
            (i(903), Value::Shared { slot: 4, value: Box::new(list(vec![i(13)])) }),
            (i(904), Value::Shared { slot: 5, value: Box::new(b("interned preset")) }),
            // Wrapped values whose payload is only a Ref.
            (b("backgroundStates2"), w(Value::Ref(1))),
            (b("backgroundOrder2"), w(Value::Ref(2))),
            (b("flagStates2"), w(Value::Ref(1))),
            (b("flagOrder2"), w(Value::Ref(2))),
            (
                b("overviewProfilePresets"),
                w(dict(vec![(
                    Value::Ref(5),
                    dict(vec![
                        (b("groups"), list(vec![i(1)])),
                        // Bare Refs: no timestamp wrapper at all.
                        (b("filteredStates"), Value::Ref(3)),
                        (b("alwaysShownStates"), Value::Ref(4)),
                    ]),
                )])),
            ),
            (b("activeOverviewPreset"), w(Value::Ref(5))),
            (
                b("tabsettings_new"),
                w(dict(vec![(
                    i(0),
                    dict(vec![
                        (Value::StrTable(52), b("  *  ")),
                        (b("overview"), Value::Ref(5)),
                        (b("bracket"), Value::Ref(5)),
                        (b("color"), Value::None),
                        (b("tabColumns"), col_list(&["ICON", "NAME"])),
                        (b("tabColumnOrder"), col_list(&OVERVIEW_COLUMNS)),
                    ]),
                )])),
            ),
        ]),
    )])
}

// --------------------------------------------------------- decode-only

/// Hand-authored wire bytes for the two deprecated string opcodes. The decoder
/// accepts both; the encoder canonically re-emits their content as BUFFER, so
/// these bytes cannot survive a byte-identity check and live in a directory the
/// gate skips. This is the only coverage those two decoder arms have.
fn deprecated_strings() -> Vec<u8> {
    const DICT: u8 = 0x16;
    const BUFFER: u8 = 0x13;
    const STRING: u8 = 0x10;
    const STRINGL: u8 = 0x0d;

    let mut out = vec![0x7e, 0, 0, 0, 0]; // magic + shared count 0
    out.push(DICT);
    out.push(2); // two entries
    // Wire order per entry is value, then key.
    out.push(STRING);
    out.push(13);
    out.extend(b"legacy string");
    out.push(BUFFER);
    out.push(10);
    out.extend(b"string_key");

    out.push(STRINGL);
    out.push(14);
    out.extend(b"legacy stringl");
    out.push(BUFFER);
    out.push(11);
    out.extend(b"stringl_key");
    out
}

// ---------------------------------------------------------------- main

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic")
}

/// Encode, then prove the committed bytes decode back to exactly this tree and
/// re-encode to exactly these bytes. A fixture that cannot round-trip would turn
/// the corpus gates red for a reason that has nothing to do with the code under
/// test, so it never gets written.
fn write(dir: &Path, name: &str, tree: &Value) {
    let bytes = encode(tree).unwrap_or_else(|e| panic!("{name}: encode failed: {e}"));
    let back = decode(&bytes).unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
    assert!(back.bits_eq(tree), "{name}: decode(encode(tree)) != tree");
    let again = encode(&back).unwrap_or_else(|e| panic!("{name}: re-encode failed: {e}"));
    assert_eq!(again, bytes, "{name}: re-encode is not byte-identical");
    std::fs::create_dir_all(dir).expect("create fixture dir");
    std::fs::write(dir.join(name), &bytes).expect("write fixture");
    println!("  {:<44} {:>7} bytes", format!("{}/{name}", dir.file_name().unwrap().to_string_lossy()), bytes.len());
}

fn main() {
    let root = root();
    let codec = root.join("codec");
    let profile = root.join("profile/settings_Default");
    let decode_only = root.join("decode-only");

    println!("writing synthetic corpus to {}", root.display());

    write(&codec, "scalars.dat", &reshare(&codec_scalars()));
    write(&codec, "containers.dat", &reshare(&codec_containers()));
    // Authored sharing: reshare would renumber it into identity order.
    write(&codec, "sharing.dat", &codec_sharing());
    write(&codec, "objects.dat", &reshare(&codec_objects()));
    write(&codec, "odd_keys.dat", &reshare(&codec_odd_keys()));

    write(&profile, "core_char_90000001.dat", &reshare(&char_modern()));
    write(&profile, "core_char_90000002.dat", &reshare(&char_minimal()));
    write(&profile, "core_char_90000003.dat", &reshare(&char_legacy()));
    write(&profile, "core_user_80000001.dat", &reshare(&user_modern()));
    write(&profile, "core_user_80000002.dat", &reshare(&user_clean()));
    write(&profile, "core_user_80000003.dat", &reshare(&user_legacy()));
    write(&profile, "core_user_80000004.dat", &user_interned());

    let bytes = deprecated_strings();
    decode(&bytes).expect("decode-only fixture must still decode");
    std::fs::create_dir_all(&decode_only).expect("create fixture dir");
    std::fs::write(decode_only.join("deprecated_strings.dat"), &bytes).expect("write fixture");
    println!("  {:<44} {:>7} bytes", "decode-only/deprecated_strings.dat", bytes.len());

    println!("done");
}
