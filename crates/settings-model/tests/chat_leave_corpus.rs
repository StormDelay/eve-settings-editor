//! Real-data guard for the leave rule (`chat.rs::is_leavable`). The rule
//! refuses anything that is not `player_*` or `private_*`; this pins that the
//! corpus holds no THIRD kind of user channel the rule would silently refuse,
//! and that `chatPlayerChannelsJoined` is keyed the way `leave_chat_account`
//! matches it.

mod common;

use blue_marshal::Value;
use settings_model::{chat_channel_keys, is_leavable};

/// Server-assigned channels. A key outside this set that is not leavable is a
/// shape nobody has classified yet.
const STANDING: [&str; 7] = ["local", "corp", "alliance", "fleet", "incursion", "invasion", "faction"];

/// System channels the server manages, keyed `system_<ids>` — Rookie Help is
/// `system_200001_200002` (corpus, 2026-07-12). Refused like the standing
/// ones: whether leaving one in the file sticks has not been tested.
const REFUSED_PREFIXES: [&str; 1] = ["system_"];

#[test]
fn every_chat_channel_key_is_leavable_or_standing() {
    let mut unknown = std::collections::BTreeSet::new();
    let mut seen = 0usize;
    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        for key in chat_channel_keys(&doc) {
            seen += 1;
            let known = STANDING.contains(&key.as_str()) || REFUSED_PREFIXES.iter().any(|p| key.starts_with(p));
            if !is_leavable(&format!("chatchannel_{key}")) && !known {
                unknown.insert(key);
            }
        }
    }
    eprintln!("{seen} chatchannels rows");
    assert!(seen > 0, "no chatchannels rows read at all — the reader broke");
    assert!(unknown.is_empty(), "unclassified channel keys: {unknown:?}");
}

fn key_text(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}

/// The dict under `(timestamp, dict)` or a bare dict.
fn inner_dict(v: &Value) -> Option<&Vec<(Value, Value)>> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(t) => t.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
        _ => None,
    }
}

#[test]
fn chat_player_channels_joined_is_keyed_by_player_keys() {
    let (mut bad, mut seen) = (std::collections::BTreeSet::new(), 0usize);
    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let Value::Dict(top) = blue_marshal::inline(&doc) else { continue };
        let Some((_, Value::Dict(ui))) = top.iter().find(|(k, _)| key_text(k).as_deref() == Some("ui")) else { continue };
        let Some((_, joined)) = ui.iter().find(|(k, _)| key_text(k).as_deref() == Some("chatPlayerChannelsJoined")) else { continue };
        for (k, _) in inner_dict(joined).into_iter().flatten() {
            seen += 1;
            let k = key_text(k).unwrap_or_else(|| format!("{k:?}"));
            if !k.starts_with("player_") {
                bad.insert(k);
            }
        }
    }
    eprintln!("{seen} chatPlayerChannelsJoined entries");
    assert!(bad.is_empty(), "chatPlayerChannelsJoined keys not shaped player_*: {bad:?}");
}
