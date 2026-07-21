// Throwaway research tool (NOT committed): decode core_user file(s), print the
// largest overview preset(s) and their group IDs. Used to extract EVE's built-in
// "General: all" default preset group set as the overview-catalog ground truth.
use std::collections::BTreeSet;

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
