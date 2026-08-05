// Throwaway research tool: harvest EVE's overview-pack colour palette
// (name -> RGBA) from the corpus. A file whose account imported a pack keeps
// the pack verbatim under overview->restoreData->data (pack vocabulary,
// including stateColorsNameList) and the RGBA EVE derived from it under
// overview->stateColors. Joining the two gives the palette.
//
// usage: cargo run -p settings-model --bin pack_palette -- <corpus-dir>
use std::collections::BTreeMap;
use std::path::Path;
use blue_marshal::Value;

fn dict<'a>(v: &'a Value, key: &[u8]) -> Option<&'a Value> {
    let Value::Dict(d) = v else { return None };
    d.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b.as_slice() == key)).map(|(_, v)| v)
}

fn inner(v: &Value) -> &Value {
    match v {
        Value::Tuple(items) => items.iter().find(|e| matches!(e, Value::Dict(_) | Value::List(_))).unwrap_or(v),
        other => other,
    }
}

fn walk(path: &Path, out: &mut BTreeMap<String, [String; 4]>) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let Ok(v) = blue_marshal::decode(&bytes) else { return };
    let flat = blue_marshal::inline(&v);
    let Some(ov) = dict(&flat, b"overview") else { return };
    let ov = inner(ov);

    // names: restoreData -> data -> stateColorsNameList
    let mut names: BTreeMap<i64, String> = BTreeMap::new();
    if let Some(rd) = dict(ov, b"restoreData") {
        if let Some(data) = dict(inner(rd), b"data") {
            if let Some(Value::List(list)) = dict(inner(data), b"stateColorsNameList") {
                for e in list {
                    let Value::Tuple(kv) = e else { continue };
                    let [k, val] = kv.as_slice() else { continue };
                    let (Value::Bytes(k), Value::Bytes(val)) = (k, val) else { continue };
                    let k = String::from_utf8_lossy(k).into_owned();
                    let Some(id) = k.strip_prefix("background_").and_then(|n| n.parse::<i64>().ok()) else { continue };
                    names.insert(id, String::from_utf8_lossy(val).into_owned());
                }
            }
        }
    }
    if names.is_empty() { return; }

    // rgba: stateColors -> {(b"background", id): (f,f,f,f)}
    let Some(sc) = dict(ov, b"stateColors") else { return };
    let Value::Dict(entries) = inner(sc) else { return };
    for (k, val) in entries {
        let Value::Tuple(kp) = k else { continue };
        let [Value::Bytes(surface), Value::Int(id)] = kp.as_slice() else { continue };
        if surface.as_slice() != b"background" { continue }
        let Value::Tuple(rgba) = val else { continue };
        let parts: Vec<String> = rgba.iter().map(|c| match c {
            Value::Float(f) => format!("{f:?}"),
            Value::Int(i) => format!("{:?}", *i as f64),
            _ => "?".into(),
        }).collect();
        let [r, g, b, a] = parts.as_slice() else { continue };
        if let Some(name) = names.get(id) {
            out.insert(name.clone(), [r.clone(), g.clone(), b.clone(), a.clone()]);
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("usage: pack_palette <corpus-dir>");
    let mut out = BTreeMap::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(p) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&p) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() { stack.push(path); }
            else if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("core_user_")) {
                walk(&path, &mut out);
            }
        }
    }
    for (name, [r, g, b, a]) in out {
        println!("(\"{name}\", [{r}, {g}, {b}, {a}]),");
    }
}
