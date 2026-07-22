// Throwaway research tool (NOT committed): decode core_user file(s), print the
// largest overview preset(s) and their group IDs. Used to extract EVE's built-in
// "General: all" default preset group set as the overview-catalog ground truth.
//
// BLOBS=1 mode: dump every built-in default preset's raw
// {groups, filteredStates, alwaysShownStates} as TSV, feeding
// tools/gen-default-presets.py (see task-1-brief.md).
use std::collections::BTreeSet;

fn is_default_key(k: &str) -> bool {
    k.strip_prefix("DefaultPreset_").map_or(false, |n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        || k.to_ascii_lowercase().starts_with("default")
}

fn ints(v: &blue_marshal::Value) -> Vec<i64> {
    match v { blue_marshal::Value::List(l) =>
        l.iter().filter_map(|e| if let blue_marshal::Value::Int(n) = e { Some(*n) } else { None }).collect(),
        _ => Vec::new() }
}

fn dump_default_blobs(v: &blue_marshal::Value) {
    use blue_marshal::Value;
    let flat = blue_marshal::inline(v);
    let Value::Dict(root) = &flat else { return };
    let Some((_, ov)) = root.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"overview")) else { return };
    let Value::Dict(ovd) = ov else { return };
    let Some((_, p)) = ovd.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"overviewProfilePresets")) else { return };
    // (timestamp, dict) or bare dict
    let inner = match p { Value::Tuple(items) => items.iter().find_map(|e| if let Value::Dict(d) = e { Some(d) } else { None }),
                          Value::Dict(d) => Some(d), _ => None };
    let Some(pd) = inner else { return };
    for (k, blob) in pd {
        let Value::Bytes(kb) = k else { continue };
        let key = String::from_utf8_lossy(kb).into_owned();
        if !is_default_key(&key) { continue; }
        let Value::Dict(fields) = blob else { continue };
        let field = |name: &[u8]| fields.iter().find(|(fk, _)| matches!(fk, Value::Bytes(b) if b.as_slice() == name)).map(|(_, fv)| ints(fv)).unwrap_or_default();
        let g = field(b"groups"); let fs = field(b"filteredStates"); let a = field(b"alwaysShownStates");
        let csv = |xs: &[i64]| xs.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        println!("BLOB\t{}\t{}\t{}\t{}", key, csv(&g), csv(&fs), csv(&a));
    }
}

fn main() {
    let mut union: BTreeSet<i64> = BTreeSet::new();
    let mut best_name = String::new();
    let mut best: Vec<i64> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths: Vec<String> = if args.is_empty() {
        use std::io::BufRead;
        std::io::stdin().lock().lines().map_while(Result::ok).collect()
    } else {
        args
    };
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(v) = blue_marshal::decode(&bytes) else { continue };
        if std::env::var("BLOBS").is_ok() { dump_default_blobs(&v); continue; }
        let cols = settings_model::project_overview(&v, None);
        if std::env::var("DIAG").is_ok() {
            eprintln!("FILE {} : {} presets, {} tabs", path, cols.presets.len(), cols.tabs.len());
            let names: Vec<String> = cols.presets.iter().map(|p| format!("{}({})", p.name, p.groups.len())).collect();
            eprintln!("  presets: {}", names.join(", "));
            for t in &cols.tabs {
                eprintln!("  tab[{}] name={:?} preset={:?} in_presets={}", t.index, t.name, t.preset, cols.presets.iter().any(|p| p.name == t.preset));
            }
        }
        for p in &cols.presets {
            for &g in &p.groups {
                union.insert(g);
            }
            if p.groups.len() > best.len() {
                best = p.groups.clone();
                best_name = p.name.clone();
            }
        }
    }
    println!("largest single preset: {} -> {} groups", best_name, best.len());
    println!("UNION across all presets/files: {} groups", union.len());
    let all: Vec<String> = union.into_iter().map(|g| g.to_string()).collect();
    println!("UNION_IDS=[{}]", all.join(","));
    let best_s: Vec<String> = best.iter().map(|g| g.to_string()).collect();
    println!("BEST_IDS=[{}]", best_s.join(","));
}
