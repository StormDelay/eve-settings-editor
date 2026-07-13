use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    /// Encoded canonically by magnitude: -1/0/1 → MINUSONE/ZERO/ONE, then the
    /// narrowest of INT8/INT16/INT32/INT64. Corpus-proven exact: the client
    /// never writes a non-minimal width (int_nonminimal = 0 over 5022 files).
    Int(i64),
    /// LONG (0x2F) payload: raw little-endian two's-complement bytes.
    Long(Vec<u8>),
    /// Encoded canonically: exact +0.0 bit pattern → FLOAT0, else FLOAT
    /// (−0.0 has a different bit pattern and stays FLOAT). Corpus-proven:
    /// FLOAT never carries +0.0 (float_pluszero = 0).
    Float(f64),
    /// Byte string. Encoded canonically by length: 0 → STRING0, 1 → STRING1,
    /// else BUFFER. Corpus-proven: BUFFER never holds ≤ 1 bytes and the
    /// deprecated STRING/STRINGL opcodes never occur.
    Bytes(Vec<u8>),
    /// Text stored as UTF8 (0x2E) — the dominant wire form (11M occurrences).
    Str(String),
    /// Text stored in the UCS-2 family, encoded by UTF-16 unit count:
    /// 0 → UNICODE0, 1 → UNICODE1, n → UNICODE. Separate from `Str` because
    /// the client emits BOTH UTF8("") and UNICODE0 — content alone cannot
    /// recover the opcode.
    StrUcs2(String),
    /// STRINGR (0x11): index 1..=255 into [`crate::string_table::STRING_TABLE`].
    /// Kept as the index because the client also writes table-content strings
    /// as UTF8 (1269 corpus collisions) — content alone cannot recover the
    /// opcode.
    StrTable(u8),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    /// Entries in wire order (each entry is encoded value-first on the wire;
    /// normalized to (key, value) here).
    Dict(Vec<(Value, Value)>),
    /// STREAM (0x2B): the payload is a complete nested marshal stream,
    /// decoded recursively and re-encoded recursively.
    Stream(Box<Value>),
    /// GLOBAL (0x02): a dotted Python type/function name.
    Global(Vec<u8>),
    /// INSTANCE (0x17): class-name object, then exactly one state object.
    Instance { class: Box<Value>, state: Box<Value> },
    /// REDUCE (0x22): ctor tuple `(callable, args[, state])`, then the
    /// MARK-terminated list items, then the MARK-terminated (key, value)
    /// pairs — the wire framing kept verbatim (both empty in every corpus
    /// occurrence).
    Reduce { ctor: Box<Value>, items: Vec<Value>, pairs: Vec<(Value, Value)> },
    /// A SHARED_FLAG-ed store: wraps exactly the node whose opcode carried
    /// the flag; `slot` is the 1-based tail-map slot it was stored into.
    /// Slots are explicit because corpus tail maps are heavily non-identity
    /// (963,660 out-of-order entries).
    Shared { slot: u32, value: Box<Value> },
    /// REF (0x1B): points at the `Shared` node with the same slot number,
    /// which must appear earlier in the stream.
    Ref(u32),
}

impl Value {
    /// Peel a `Shared` wrapper (if any) to reach the stored node — the
    /// wrapper is wire bookkeeping, not data. Does NOT resolve `Ref`.
    pub fn unshared(&self) -> &Value {
        match self {
            Value::Shared { value, .. } => value,
            other => other,
        }
    }
}

/// Deterministic text rendering. Dict keys are sorted by their rendered
/// form so that two dumps of semantically equal data diff cleanly.
pub fn dump_text(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0, 0);
    out
}

/// `stream_depth` counts nested-stream decode attempts made by the
/// `Value::Bytes` arm below (a `Bytes` payload starting with the 0x7E magic,
/// double-marshaled settings values). That path calls `crate::decode::decode`
/// — the public entry point, which always starts a fresh decoder at depth 0
/// — so, unlike ordinary container recursion (Tuple/List/Dict/Instance/
/// Stream, which only ever mirrors a `Value` tree that `decode`'s own
/// `MAX_DEPTH` has already bounded), a chain of such payloads is
/// attacker-lengthenable independent of that guard. Bounded here with the
/// same `MAX_DEPTH`, threaded through unchanged by every other recursive
/// call and incremented only where a nested stream is actually decoded.
fn write_value(out: &mut String, v: &Value, indent: usize, stream_depth: usize) {
    let pad = "  ".repeat(indent);
    match v {
        Value::None => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Long(bytes) => {
            // The wire payload is signed little-endian two's-complement
            // (Task 4, confirmed against marshal.c's `_PyLong_FromByteArray`
            // call), but this renders it as unsigned: a Task 9 corpus-wide
            // scan of all 1116 files found zero negative Longs, so there is
            // nothing in the corpus this misrenders. Left as-is per the
            // task brief (fix only if negative Longs are observed); revisit
            // if a future corpus file ever has the top bit of the last byte
            // set.
            if bytes.len() <= 16 {
                let mut buf = [0u8; 16];
                buf[..bytes.len()].copy_from_slice(bytes);
                let _ = write!(out, "{}L", u128::from_le_bytes(buf));
            } else {
                let _ = write!(out, "longhex:{}", hex(bytes));
            }
        }
        Value::Float(f) => {
            let _ = write!(out, "{f:?}");
        }
        Value::Str(s) => {
            let _ = write!(out, "{s:?}");
        }
        Value::StrUcs2(s) => {
            out.push('u');
            let _ = write!(out, "{s:?}");
        }
        Value::StrTable(idx) => {
            let _ = write!(
                out,
                "t{idx}:{:?}",
                crate::string_table::STRING_TABLE[*idx as usize]
            );
        }
        Value::Bytes(b) => {
            // Double-marshaled settings values: a Bytes payload that is
            // itself a complete marshal stream (starts with the 0x7E magic).
            // `decode` never does this automatically — the bytes stay exact
            // and lossless — but a readable dump is worth attempting one
            // decode of the payload; fall back to plain rendering if it
            // doesn't parse (0x7E can just as well be an ordinary byte).
            if b.first() == Some(&0x7E) && stream_depth < crate::decode::MAX_DEPTH {
                if let Ok(inner) = crate::decode::decode(b) {
                    out.push_str("stream?");
                    write_value(out, &inner, indent, stream_depth + 1);
                    return;
                }
            }
            write_bytes_body(out, b, "b", "hex:");
        }
        Value::Global(name) => write_bytes_body(out, name, "global:", "global-hex:"),
        Value::Tuple(items) => write_seq(out, items, indent, '(', ')', stream_depth),
        Value::List(items) => write_seq(out, items, indent, '[', ']', stream_depth),
        Value::Dict(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            // Render each key through the depth-threaded path rather than
            // the public `dump_text` entry point: `dump_text` always starts
            // a fresh `write_value` at `stream_depth = 0`, so a chain of
            // dicts nested via their *keys* (each key a Bytes payload that
            // decodes to the next dict) would get a brand-new depth budget
            // at every hop, bypassing the nested-stream guard entirely.
            // Indentation still starts at 0 for the sort key (matching
            // `dump_text`'s own behavior), but `stream_depth` is the ambient
            // one, not reset.
            let mut rendered: Vec<(String, &Value)> = entries
                .iter()
                .map(|(k, v)| {
                    let mut key_buf = String::new();
                    write_value(&mut key_buf, k, 0, stream_depth);
                    (key_buf, v)
                })
                .collect();
            rendered.sort_by(|a, b| a.0.cmp(&b.0));
            out.push_str("{\n");
            for (key, val) in rendered {
                let _ = write!(out, "{pad}  {key}: ");
                write_value(out, val, indent + 1, stream_depth);
                out.push('\n');
            }
            let _ = write!(out, "{pad}}}");
        }
        Value::Stream(inner) => {
            out.push_str("stream:");
            write_value(out, inner, indent, stream_depth);
        }
        Value::Instance { class, state } => {
            out.push_str("instance{\n");
            let _ = write!(out, "{pad}  class: ");
            write_value(out, class, indent + 1, stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}  state: ");
            write_value(out, state, indent + 1, stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}}}");
        }
        Value::Reduce { ctor, items, pairs } => {
            out.push_str("reduce{\n");
            let _ = write!(out, "{pad}  ctor: ");
            write_value(out, ctor, indent + 1, stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}  items: ");
            write_seq(out, items, indent + 1, '[', ']', stream_depth);
            out.push('\n');
            let _ = write!(out, "{pad}  pairs: ");
            if pairs.is_empty() {
                out.push_str("{}");
            } else {
                // Wire order, NOT sorted — this is a fidelity view of the
                // REDUCE iterator tail, unlike Dict's sorted diff view.
                out.push_str("{\n");
                for (k, v) in pairs {
                    let _ = write!(out, "{pad}    ");
                    write_value(out, k, indent + 2, stream_depth);
                    out.push_str(": ");
                    write_value(out, v, indent + 2, stream_depth);
                    out.push('\n');
                }
                let _ = write!(out, "{pad}  }}");
            }
            out.push('\n');
            let _ = write!(out, "{pad}}}");
        }
        Value::Shared { slot, value } => {
            let _ = write!(out, "shared[{slot}]:");
            write_value(out, value, indent, stream_depth);
        }
        Value::Ref(slot) => {
            let _ = write!(out, "ref[{slot}]");
        }
    }
}

/// Shared rendering for byte strings (`Bytes` and `Global`): printable ASCII
/// (and empty) renders quoted with `quoted_prefix` (embedded `"` and `\`
/// escaped with a backslash); anything else renders as hex with `hex_prefix`,
/// so `Bytes` and `Global` stay distinguishable in both branches.
fn write_bytes_body(out: &mut String, b: &[u8], quoted_prefix: &str, hex_prefix: &str) {
    if b.iter().all(|c| (0x20..0x7F).contains(c)) {
        out.push_str(quoted_prefix);
        out.push('"');
        for &c in b {
            if c == b'"' || c == b'\\' {
                out.push('\\');
            }
            out.push(c as char);
        }
        out.push('"');
    } else {
        let _ = write!(out, "{hex_prefix}{}", hex(b));
    }
}

fn write_seq(
    out: &mut String,
    items: &[Value],
    indent: usize,
    open: char,
    close: char,
    stream_depth: usize,
) {
    let pad = "  ".repeat(indent);
    if items.is_empty() {
        out.push(open);
        out.push(close);
        return;
    }
    out.push(open);
    out.push('\n');
    for item in items {
        out.push_str(&pad);
        out.push_str("  ");
        write_value(out, item, indent + 1, stream_depth);
        out.push('\n');
    }
    out.push_str(&pad);
    out.push(close);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_scalars() {
        assert_eq!(dump_text(&Value::None), "None");
        assert_eq!(dump_text(&Value::Bool(true)), "True");
        assert_eq!(dump_text(&Value::Int(-7)), "-7");
        assert_eq!(dump_text(&Value::Float(2.5)), "2.5");
        assert_eq!(dump_text(&Value::Str("abc".into())), "\"abc\"");
    }

    #[test]
    fn dump_bytes_printable_and_hex() {
        assert_eq!(dump_text(&Value::Bytes(b"overview".to_vec())), "b\"overview\"");
        assert_eq!(dump_text(&Value::Bytes(vec![0x00, 0xFF])), "hex:00ff");
    }

    #[test]
    fn dump_bytes_escapes_quotes_and_backslashes() {
        assert_eq!(
            dump_text(&Value::Bytes(b"a\"b\\c".to_vec())),
            r#"b"a\"b\\c""#
        );
    }

    #[test]
    fn dump_global_nonprintable_uses_global_hex_prefix() {
        assert_eq!(dump_text(&Value::Global(vec![0x00, 0xFF])), "global-hex:00ff");
        assert_eq!(dump_text(&Value::Bytes(vec![0x00, 0xFF])), "hex:00ff");
    }

    #[test]
    fn dump_bytes_attempts_nested_stream_decode() {
        // A Bytes payload that is itself a complete, valid marshal stream
        // (magic 0x7E, shared_count 0, one ONE opcode) — double-marshaled
        // settings values look like this. Rendered decoded, not as hex/text.
        let nested = vec![0x7E, 0, 0, 0, 0, 0x09];
        assert_eq!(dump_text(&Value::Bytes(nested)), "stream?1");

        // Starts with 0x7E but is not a valid stream (too short to hold the
        // shared-count word) — falls back to plain byte rendering.
        assert_eq!(dump_text(&Value::Bytes(vec![0x7E])), "b\"~\"");
    }

    #[test]
    fn dump_bytes_bounds_nested_stream_recursion_depth() {
        // Regression test for unbounded recursion in the nested-stream
        // decode attempt above: it calls `crate::decode::decode` — which
        // always starts a fresh decoder at depth 0 — so a chain of `Bytes`
        // payloads, each wrapping the next as a valid marshal stream, used to
        // recurse once per chain link with no bound at all, independent of
        // `decode`'s own MAX_DEPTH guard. A crafted file with such a chain
        // could drive this to a stack-overflow process abort.
        //
        // A genuine stack-overflow demonstration aborts the whole process
        // (uncatchable — not a normal panic), which `cargo test` can't
        // capture as a clean pass/fail on Windows, so this demonstrates the
        // same fact a different way, per this task's documented fallback:
        // a chain long enough to prove the bound (well beyond MAX_DEPTH) but
        // shallow enough that even the unbounded pre-fix code completes
        // normally (no crash) — before the fix it decodes the *entire*
        // chain (all `chain_len` links, leaf included); after the fix it
        // stops attempting nested decodes at exactly MAX_DEPTH links and
        // falls back to raw rendering for the rest, so the leaf is never
        // reached.
        let chain_len = 200; // well beyond MAX_DEPTH (64)
        let leaf = b"LEAF_MARKER_UNIQUE";
        let payload = nested_stream_chain(chain_len, leaf);

        let text = dump_text(&Value::Bytes(payload));

        assert_eq!(
            text.matches("stream?").count(),
            crate::decode::MAX_DEPTH,
            "nested-stream decoding must stop at MAX_DEPTH, not recurse the whole chain"
        );
        assert!(
            !text.contains("LEAF_MARKER_UNIQUE"),
            "leaf beyond MAX_DEPTH must never be reached"
        );
    }

    /// Build a chain of `depth` nested marshal streams, each a BUFFER opcode
    /// (0x13) whose payload is the next level down, bottoming out at `leaf`
    /// (which must not itself start with 0x7E, so the chain has a clean
    /// terminator). Built in one O(depth) pass — level sizes precomputed
    /// bottom-up, then headers written top-down directly into position —
    /// rather than by repeated wrapping, which would re-copy the whole
    /// growing payload at every level (O(depth^2)).
    fn nested_stream_chain(depth: usize, leaf: &[u8]) -> Vec<u8> {
        const HEADER_LEN: usize = 5 + 1 + 5; // stream header + BUFFER opcode + extended length
        let mut sizes = Vec::with_capacity(depth + 1);
        sizes.push(leaf.len());
        for i in 1..=depth {
            sizes.push(sizes[i - 1] + HEADER_LEN);
        }
        let mut buf = Vec::with_capacity(sizes[depth]);
        for i in (1..=depth).rev() {
            buf.extend_from_slice(&[0x7E, 0, 0, 0, 0, 0x13, 0xFF]);
            buf.extend_from_slice(&(sizes[i - 1] as u32).to_le_bytes());
        }
        buf.extend_from_slice(leaf);
        buf
    }

    #[test]
    fn dump_dict_key_nested_stream_bypasses_depth_guard_via_dump_text() {
        // Regression test for a review finding on the fix above: the `Dict`
        // arm's key-rendering closure calls the *public* `dump_text` entry
        // point (not the depth-threaded `write_value`), which always starts
        // a fresh `write_value` at `stream_depth = 0`. Decoded dict keys are
        // ordinary `Value`s (typically `Bytes`), so a chain of
        // `Dict { key: Bytes(nested 0x7E stream wrapping the next dict),
        // value: None }` gets a brand-new depth budget at every hop — the
        // `stream_depth < MAX_DEPTH` guard in the `Bytes` arm is checked
        // each time, but against a depth that was just reset to 0, so it
        // never actually stops the chain.
        //
        // Same RED/GREEN approach as
        // `dump_bytes_bounds_nested_stream_recursion_depth`: a genuine
        // stack-overflow demonstration aborts the whole process
        // (uncatchable, not capturable as a clean `cargo test` pass/fail on
        // Windows), so this uses a chain well beyond MAX_DEPTH but shallow
        // enough that even the unbounded buggy code completes normally.
        let chain_len = 200; // well beyond MAX_DEPTH (64)
        let leaf = b"DICT_KEY_LEAF_MARKER";
        let chain = nested_dict_key_chain(chain_len, leaf);
        let top = Value::Dict(vec![(Value::Bytes(chain), Value::None)]);

        let text = dump_text(&top);

        assert_eq!(
            text.matches("stream?").count(),
            crate::decode::MAX_DEPTH,
            "dict-key nested-stream decoding must stop at MAX_DEPTH, not restart at 0 every hop"
        );
        assert!(
            !text.contains("DICT_KEY_LEAF_MARKER"),
            "leaf beyond MAX_DEPTH must never be reached"
        );
    }

    /// Build a chain of `depth` nested marshal streams, each a DICT with one
    /// entry `{ value: None, key: <BUFFER wrapping the next level down> }`,
    /// bottoming out at `leaf` (which must not itself start with 0x7E, so
    /// the chain has a clean terminator). Mirrors `nested_stream_chain`'s
    /// O(depth) flat-buffer construction (fixed-size prefix per level,
    /// precomputed sizes bottom-up, then written top-down directly into
    /// position) — just with a bigger per-level prefix (DICT opcode + count
    /// + NONE value ahead of the BUFFER key), since here the nesting is
    /// through a dict key rather than a bare BUFFER payload.
    fn nested_dict_key_chain(depth: usize, leaf: &[u8]) -> Vec<u8> {
        // stream header + DICT op + count(=1) + NONE value + BUFFER key op + ext length
        const HEADER_LEN: usize = 5 + 1 + 1 + 1 + 1 + 5;
        let mut sizes = Vec::with_capacity(depth + 1);
        sizes.push(leaf.len());
        for i in 1..=depth {
            sizes.push(sizes[i - 1] + HEADER_LEN);
        }
        let mut buf = Vec::with_capacity(sizes[depth]);
        for i in (1..=depth).rev() {
            buf.extend_from_slice(&[0x7E, 0, 0, 0, 0, 0x16, 0x01, 0x01, 0x13, 0xFF]);
            buf.extend_from_slice(&(sizes[i - 1] as u32).to_le_bytes());
        }
        buf.extend_from_slice(leaf);
        buf
    }

    #[test]
    fn dump_global_and_instance() {
        assert_eq!(
            dump_text(&Value::Global(b"__builtin__.set".to_vec())),
            "global:\"__builtin__.set\""
        );
        let inst = Value::Instance {
            class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
            state: Box::new(Value::Int(1)),
        };
        assert_eq!(
            dump_text(&inst),
            "instance{\n  class: b\"M.Cls\"\n  state: 1\n}"
        );
    }

    #[test]
    fn dump_new_string_variants() {
        assert_eq!(dump_text(&Value::StrUcs2("hi".into())), "u\"hi\"");
        // Expected text derives from the table itself so the test does not
        // hardcode table content.
        assert_eq!(
            dump_text(&Value::StrTable(7)),
            format!("t7:{:?}", crate::string_table::STRING_TABLE[7])
        );
    }

    #[test]
    fn dump_shared_ref_and_reduce() {
        let v = Value::Tuple(vec![
            Value::Shared { slot: 2, value: Box::new(Value::List(vec![])) },
            Value::Ref(2),
        ]);
        assert_eq!(dump_text(&v), "(\n  shared[2]:[]\n  ref[2]\n)");
        let r = Value::Reduce {
            ctor: Box::new(Value::Global(b"M.f".to_vec())),
            items: vec![Value::Int(1)],
            pairs: vec![(Value::Int(0), Value::Int(1))],
        };
        assert_eq!(
            dump_text(&r),
            "reduce{\n  ctor: global:\"M.f\"\n  items: [\n    1\n  ]\n  pairs: {\n    0: 1\n  }\n}"
        );
    }

    #[test]
    fn unshared_peels_exactly_one_wrapper() {
        let inner = Value::Int(5);
        let shared = Value::Shared { slot: 1, value: Box::new(inner.clone()) };
        assert_eq!(shared.unshared(), &inner);
        assert_eq!(inner.unshared(), &inner);
    }

    #[test]
    fn dump_dict_sorted_by_key_rendering() {
        let d = Value::Dict(vec![
            (Value::Bytes(b"zeta".to_vec()), Value::Int(1)),
            (Value::Bytes(b"alpha".to_vec()), Value::Int(2)),
        ]);
        let text = dump_text(&d);
        let alpha = text.find("alpha").unwrap();
        let zeta = text.find("zeta").unwrap();
        assert!(alpha < zeta, "dict dump must sort keys for stable diffs");
    }

    #[test]
    fn dump_nested_containers_indent() {
        let v = Value::Tuple(vec![Value::Int(1), Value::List(vec![Value::None])]);
        assert_eq!(dump_text(&v), "(\n  1\n  [\n    None\n  ]\n)");
    }
}
