// Harvest the neocom button catalog (id -> btnType, iconPath) from the corpus,
// as JSON for app/src/lib/data/neocom-buttons.json. A button's btnType and icon
// are attributes of what the button IS, not of the character (spec §2), so the
// most common pairing per id is the canonical one.
//
// usage: cargo run -p settings-model --bin neocom_catalog -- <corpus-dir> > app/src/lib/data/neocom-buttons.json
use blue_marshal::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn child<'a>(v: &'a Value, key: &[u8]) -> Option<&'a Value> {
    let Value::Dict(d) = v else { return None };
    d.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b.as_slice() == key)).map(|(_, v)| v)
}

fn payload(v: &Value) -> &Value {
    match v {
        Value::Tuple(t) if t.len() == 2 => &t[1],
        other => other,
    }
}

fn bytes(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Tuple(t) => t.first().and_then(bytes), // the Tuple(bytes, None) id shape
        _ => None,
    }
}

/// id -> (btnType, iconPath) -> count
type Tally = BTreeMap<String, BTreeMap<(i64, String), usize>>;

fn visit(v: &Value, out: &mut Tally) {
    let Value::Instance { state, .. } = v else { return };
    let (Some(id), Some(bt), Some(icon)) = (
        child(state, b"id").and_then(bytes),
        child(state, b"btnType").and_then(|v| if let Value::Int(i) = v { Some(*i) } else { None }),
        child(state, b"iconPath").and_then(bytes),
    ) else { return };
    if id.is_empty() { return }
    *out.entry(id).or_default().entry((bt, icon)).or_default() += 1;
    if let Some(Value::List(kids) | Value::Tuple(kids)) = child(state, b"children") {
        for k in kids { visit(k, out); }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: neocom_catalog <corpus-dir>");
    let mut files = Vec::new();
    collect(Path::new(&dir), &mut files);
    let mut tally: Tally = BTreeMap::new();
    for p in &files {
        if !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("core_char_")) { continue }
        let Ok(raw) = std::fs::read(p) else { continue };
        let Ok(decoded) = blue_marshal::decode(&raw) else { continue };
        let v = blue_marshal::inline(&decoded); // resolve Shared/Ref before reading
        let Some(ui) = child(&v, b"ui") else { continue };
        for key in [b"neocomButtonRawData".as_slice(), b"neocomButtonRawDataOriginal"] {
            if let Some(bar) = child(ui, key) {
                if let Value::List(l) | Value::Tuple(l) = payload(bar) {
                    for b in l { visit(b, &mut tally); }
                }
            }
        }
    }
    println!("[");
    let rows: Vec<String> = tally
        .iter()
        .filter_map(|(id, variants)| {
            let ((bt, icon), _) = variants.iter().max_by_key(|(_, n)| **n)?;
            Some(format!("  {{ \"id\": {}, \"btnType\": {bt}, \"iconPath\": {} }}", quote(id), quote(icon)))
        })
        .collect();
    println!("{}", rows.join(",\n"));
    println!("]");
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() { collect(&p, out); } else if p.extension().is_some_and(|x| x == "dat") { out.push(p); }
    }
}
