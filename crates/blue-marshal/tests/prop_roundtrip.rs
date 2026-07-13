//! Property-style round-trip test with a std-only PRNG (the crate takes no
//! dependencies, dev-dependencies included). Generates random fidelity-tagged
//! trees, encodes, decodes, and requires structural equality.
//!
//! Shared/Ref nodes are deliberately NOT generated: valid sharing has
//! stream-order constraints (a Ref must follow its completed Shared) that a
//! naive random generator would mostly violate; sharing is covered by the
//! encode.rs unit fixtures and by every real file in the corpus gate.

use blue_marshal::{decode, encode, Value};

/// xorshift64* — deterministic, seedable, no dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn gen_string(rng: &mut Rng, max_len: u64) -> String {
    // Mix of ASCII, a 2-byte UTF-8 char, a 3-byte char, and an astral
    // (surrogate-pair) char so UTF-8 byte counts and UTF-16 unit counts vary
    // independently.
    const ALPHABET: [char; 8] = ['a', 'Z', '0', ' ', '_', 'é', '☃', '🚀'];
    (0..rng.below(max_len))
        .map(|_| ALPHABET[rng.below(8) as usize])
        .collect()
}

fn gen_value(rng: &mut Rng, depth: u32) -> Value {
    // Containers only above depth 4 keeps trees small and far from MAX_DEPTH.
    let n_kinds = if depth >= 4 { 9 } else { 13 };
    match rng.below(n_kinds) {
        0 => Value::None,
        1 => Value::Bool(rng.below(2) == 0),
        // Arithmetic shift by 0..63 bits hits every canonical width bucket.
        2 => Value::Int((rng.next() as i64) >> rng.below(64)),
        3 => Value::Long((0..rng.below(6)).map(|_| rng.next() as u8).collect()),
        // Finite by construction; NaN is excluded because NaN != NaN would
        // fail the equality assert (bit-level float fidelity is proven by
        // the corpus gate instead).
        4 => Value::Float((rng.next() as i32 as f64) / 16.0),
        5 => Value::Bytes((0..rng.below(10)).map(|_| rng.next() as u8).collect()),
        6 => Value::Str(gen_string(rng, 10)),
        7 => Value::StrUcs2(gen_string(rng, 6)),
        8 => Value::StrTable((1 + rng.below(255)) as u8),
        9 => Value::Tuple((0..rng.below(4)).map(|_| gen_value(rng, depth + 1)).collect()),
        10 => Value::List((0..rng.below(4)).map(|_| gen_value(rng, depth + 1)).collect()),
        11 => Value::Dict(
            (0..rng.below(3))
                .map(|_| (gen_value(rng, depth + 1), gen_value(rng, depth + 1)))
                .collect(),
        ),
        _ => match rng.below(3) {
            0 => Value::Global(b"__builtin__.set".to_vec()),
            1 => Value::Instance {
                class: Box::new(Value::Bytes(b"utillib.KeyVal".to_vec())),
                state: Box::new(gen_value(rng, depth + 1)),
            },
            _ => Value::Reduce {
                ctor: Box::new(Value::Tuple(vec![
                    Value::Global(b"__builtin__.set".to_vec()),
                    Value::Tuple(vec![gen_value(rng, depth + 1)]),
                ])),
                items: (0..rng.below(2)).map(|_| gen_value(rng, depth + 1)).collect(),
                pairs: (0..rng.below(2))
                    .map(|_| (gen_value(rng, depth + 1), gen_value(rng, depth + 1)))
                    .collect(),
            },
        },
    }
}

#[test]
fn random_trees_roundtrip_through_encode_decode() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..2000 {
        let v = gen_value(&mut rng, 0);
        let bytes =
            encode(&v).unwrap_or_else(|e| panic!("case {case}: encode failed: {e}\n{v:?}"));
        let back =
            decode(&bytes).unwrap_or_else(|e| panic!("case {case}: decode failed: {e}\n{v:?}"));
        assert_eq!(back, v, "case {case} round-trip mismatch");
    }
}
