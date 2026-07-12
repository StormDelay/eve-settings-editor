use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, path] if cmd == "dump" => dump(Path::new(path)),
        [cmd, path] if cmd == "scan" => scan(Path::new(path)),
        _ => {
            eprintln!("usage: bmdump dump <file.dat> | bmdump scan <dir>");
            ExitCode::from(2)
        }
    }
}

fn dump(path: &Path) -> ExitCode {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match blue_marshal::decode(&data) {
        Ok(v) => {
            println!("{}", blue_marshal::dump_text(&v));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}

fn scan(dir: &Path) -> ExitCode {
    let mut files = Vec::new();
    collect(dir, &mut files);
    let (mut ok, mut failed) = (0u32, 0u32);
    for f in &files {
        let data = fs::read(f).unwrap_or_default();
        match blue_marshal::decode(&data) {
            Ok(_) => ok += 1,
            Err(e) => {
                failed += 1;
                println!("FAIL {}: {e}", f.display());
            }
        }
    }
    println!("scanned {}, ok {ok}, failed {failed}", files.len());
    if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "dat") {
            out.push(path);
        }
    }
}
