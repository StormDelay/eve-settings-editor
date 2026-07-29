//! What the pre-encode passes cost on the biggest real files. `#[ignore]`d: it
//! measures rather than asserts, needs the gitignored real corpus, and timings
//! are not a pass/fail property. Run it on demand:
//!
//! ```text
//! cargo test -p settings-model --release --test reshare_cost -- --ignored --nocapture
//! ```
//!
//! Baseline, 2026-07-30, largest real account files (~390 KB), release build:
//!
//! ```text
//!      bytes    decode    inline   reshare    encode
//!     390184       3.2       3.8      10.5       1.0   (ms)
//! ```
//!
//! A debug build is ~2.5x that (reshare ~25 ms), which is what a `cargo run`
//! session feels — worth knowing before mistaking dev-mode latency for a real
//! cost. **Conclusion: the whole-document reshare every structural editor runs
//! is not a bottleneck** — ~10 ms on the largest account file anyone has, once
//! per edit, against a 3 ms decode and a 1 ms encode. The caching, incremental
//! and subtree-scoped variants the ledger floated all buy less than they cost in
//! complexity; re-run this before reconsidering.
mod common;

use std::time::Instant;

#[test]
#[ignore = "measurement, not an assertion — needs the real corpus"]
fn measure_reshare_on_the_biggest_files() {
    let mut files: Vec<_> = common::corpus().iter().filter(|f| !f.synthetic).collect();
    if files.is_empty() {
        eprintln!("no real corpus present — nothing to measure");
        return;
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.bytes.len()));
    println!("{:>10} {:>9} {:>9} {:>9} {:>9}  file", "bytes", "decode", "inline", "reshare", "encode");
    for f in files.iter().take(12) {
        let t0 = Instant::now();
        let Ok(v) = blue_marshal::decode(&f.bytes) else { continue };
        let decode_ms = t0.elapsed().as_secs_f64() * 1e3;

        // The order a structural editor runs them in: inline the sharing away,
        // edit (nothing to edit here), reshare, encode.
        let t1 = Instant::now();
        let inlined = blue_marshal::inline(&v);
        let inline_ms = t1.elapsed().as_secs_f64() * 1e3;

        let t2 = Instant::now();
        let reshared = blue_marshal::reshare(&inlined);
        let reshare_ms = t2.elapsed().as_secs_f64() * 1e3;

        let t3 = Instant::now();
        let out = blue_marshal::encode(&reshared).expect("encode");
        let encode_ms = t3.elapsed().as_secs_f64() * 1e3;

        println!(
            "{:>10} {:>9.1} {:>9.1} {:>9.1} {:>9.1}  {} (out {} bytes)",
            f.bytes.len(),
            decode_ms,
            inline_ms,
            reshare_ms,
            encode_ms,
            f.name(),
            out.len(),
        );
    }
}
