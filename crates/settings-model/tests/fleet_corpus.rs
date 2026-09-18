//! Real-data guard for the fleet keys. Every unit test in `fleet.rs` builds its
//! own fixture, so a table naming the wrong section or key passes them all
//! while reading nothing from a real file — the class of fault `hud_corpus.rs`
//! exists for. This asserts each fleet field, each colour type and the watch
//! list actually project a value somewhere in the corpus.
//!
//! The twelve colour types beyond the client's four are carried by exactly two
//! real files (the 2026-09-18 captures on the owner's B1 account), so their bar
//! is one file, not twenty. Skips the real half silently when the corpus is not
//! checked out; the synthetic half always runs.

mod common;

use settings_model::{project_fleet, Colour, BROADCAST_TYPES};

const ENOUGH_REAL: usize = 20;
const ENOUGH_REAL_RARE: usize = 1;
const ENOUGH_SYNTHETIC: usize = 1;

/// The seven listen keys every account carries, plus the four the client
/// stores itself — the ones that can clear a twenty-file bar.
const COMMON_LISTEN: [&str; 7] = [
    "listen_HealArmor", "listen_HealShield", "listen_HealCapacitor", "listen_Target",
    "listen_HoldPosition", "listen_InPosition", "listen_NeedBackup",
];
const COMMON_COLOURS: [&str; 4] = ["HealArmor", "HealCapacitor", "HealShield", "Target"];
const CHAR_FIELDS: [&str; 4] = ["formation", "formation_size", "formation_spacing", "finder_group_only"];

fn bar(common: bool) -> usize {
    if common { ENOUGH_REAL } else { ENOUGH_REAL_RARE }
}

/// The one rare listen type this corpus ever caught toggled: `WarpTo`, in the
/// 2026-09-18 live capture. Unlike a colour (set independently of any
/// checkbox), a listen key is written only when its box is actually toggled,
/// so a rare type's real bar depends on whether anyone ever touched it — not
/// on whether the corpus contains fleet-broadcast captures at all.
const CAPTURED_LISTEN: [&str; 1] = ["listen_WarpTo"];

/// The top checkbox's field name — not a `BROADCAST_TYPES` entry, so it rides
/// its own counter rather than the per-type `listen` loop below. Carried by
/// the two 2026-09-18 captures (`listenBroadcast_ShowOwnBroadcasts`), so its
/// real bar is the same 1 as a rare listen type's.
const SHOW_OWN: &str = "listen_show_own";

/// The real-file bar for one listen key: 20 for a `COMMON_LISTEN` key (the
/// client writes these on every account regardless of toggling), 1 for
/// `listen_WarpTo` (`CAPTURED_LISTEN`, the only rare type ever caught
/// toggled), 0 for the other eight rare types — TravelTo, Event, JumpTo,
/// AlignTo, HealTarget, EnemySpotted, JumpBeacon, Location. Nobody has ever
/// toggled those eight in any file this corpus holds, so their real bar is
/// unclearable by construction; the real assertion is a no-op for them and
/// only the synthetic fixture (which sets every type) covers their key shape.
fn listen_bar(name: &str) -> usize {
    if COMMON_LISTEN.contains(&name) {
        ENOUGH_REAL
    } else if CAPTURED_LISTEN.contains(&name) {
        ENOUGH_REAL_RARE
    } else {
        0
    }
}

#[test]
fn every_account_fleet_key_reads_from_a_real_file() {
    let listen: Vec<String> = BROADCAST_TYPES.iter().map(|t| format!("listen_{t}")).collect();
    let mut listen_syn = [0usize; 16];
    let mut listen_real = [0usize; 16];
    let mut colour_syn = [0usize; 16];
    let mut colour_real = [0usize; 16];
    let mut show_own_syn = 0usize;
    let mut show_own_real = 0usize;
    let mut scanned = 0usize;

    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        scanned += 1;
        let fleet = project_fleet(None, Some(&doc));
        for (i, name) in listen.iter().enumerate() {
            let e = fleet.fields.iter().find(|e| &e.name == name).expect("field projected");
            if e.value.is_some() {
                if f.synthetic { listen_syn[i] += 1 } else { listen_real[i] += 1 }
            }
        }
        let show_own = fleet.fields.iter().find(|e| e.name == SHOW_OWN).expect("field projected");
        if show_own.value.is_some() {
            if f.synthetic { show_own_syn += 1 } else { show_own_real += 1 }
        }
        for (i, c) in fleet.colours.iter().enumerate() {
            if matches!(c.state, Colour::Set { .. } | Colour::Cleared) {
                if f.synthetic { colour_syn[i] += 1 } else { colour_real[i] += 1 }
            }
        }
    }

    for (i, name) in listen.iter().enumerate() {
        assert!(listen_syn[i] >= ENOUGH_SYNTHETIC, "{name} projected no value in any synthetic account fixture");
        if common::real_corpus_present() {
            let need = listen_bar(name.as_str());
            assert!(listen_real[i] >= need, "{name} projected a value in only {}/{scanned} real account files", listen_real[i]);
        }
    }
    assert!(show_own_syn >= ENOUGH_SYNTHETIC, "{SHOW_OWN} projected no value in any synthetic account fixture");
    if common::real_corpus_present() {
        assert!(show_own_real >= ENOUGH_REAL_RARE, "{SHOW_OWN} projected a value in only {show_own_real}/{scanned} real account files");
    }
    for (i, t) in BROADCAST_TYPES.iter().enumerate() {
        assert!(colour_syn[i] >= ENOUGH_SYNTHETIC, "colour {t} read from no synthetic account fixture");
        if common::real_corpus_present() {
            let need = bar(COMMON_COLOURS.contains(t));
            assert!(colour_real[i] >= need, "colour {t} read from only {}/{scanned} real account files", colour_real[i]);
        }
    }
}

#[test]
fn every_character_fleet_key_reads_from_a_real_file() {
    let mut field_syn = [0usize; CHAR_FIELDS.len()];
    let mut field_real = [0usize; CHAR_FIELDS.len()];
    let mut watch_syn = 0usize;
    let mut watch_real = 0usize;
    let mut scanned = 0usize;

    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        scanned += 1;
        let fleet = project_fleet(Some(&doc), None);
        for (i, name) in CHAR_FIELDS.iter().enumerate() {
            let e = fleet.fields.iter().find(|e| &e.name == name).expect("field projected");
            if e.value.is_some() {
                if f.synthetic { field_syn[i] += 1 } else { field_real[i] += 1 }
            }
        }
        if !fleet.watchlist.is_empty() {
            if f.synthetic { watch_syn += 1 } else { watch_real += 1 }
        }
    }

    for (i, name) in CHAR_FIELDS.iter().enumerate() {
        assert!(field_syn[i] >= ENOUGH_SYNTHETIC, "{name} projected no value in any synthetic character fixture");
        if common::real_corpus_present() {
            assert!(field_real[i] >= ENOUGH_REAL, "{name} projected a value in only {}/{scanned} real character files", field_real[i]);
        }
    }
    assert!(watch_syn >= ENOUGH_SYNTHETIC, "no synthetic character fixture carries a watch list");
    if common::real_corpus_present() {
        assert!(watch_real >= ENOUGH_REAL, "a watch list read from only {watch_real}/{scanned} real character files");
    }
}
