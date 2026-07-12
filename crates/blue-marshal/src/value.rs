use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Long(Vec<u8>),
    Float(f64),
    Bytes(Vec<u8>),
    Str(String),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Dict(Vec<(Value, Value)>),
    Stream(Box<Value>),
    /// GLOBAL (0x02): a dotted Python type/function name, e.g. `__builtin__.set`.
    /// Kept distinct from `Bytes` so M1's encoder can re-emit opcode 0x02
    /// rather than a string opcode.
    Global(Vec<u8>),
    /// INSTANCE (0x17): mirrors marshal.c's load order rather than modeling
    /// Python construction. `class` is the class name object; `state` holds
    /// the single state object applied via `__setstate__`/`__dict__.update`
    /// in the reference.
    Instance { class: Box<Value>, state: Vec<Value> },
}

/// Deterministic text rendering. Dict keys are sorted by their rendered
/// form so that two dumps of semantically equal data diff cleanly.
pub fn dump_text(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0);
    out
}

fn write_value(out: &mut String, v: &Value, indent: usize) {
    let pad = "  ".repeat(indent);
    match v {
        Value::None => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Long(bytes) => {
            // Interpret as unsigned LE when it fits; adjust after Task 4
            // pins down sign semantics.
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
        Value::Bytes(b) => write_bytes_body(out, b, "b"),
        Value::Global(name) => write_bytes_body(out, name, "global:"),
        Value::Tuple(items) => write_seq(out, items, indent, '(', ')'),
        Value::List(items) => write_seq(out, items, indent, '[', ']'),
        Value::Dict(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut rendered: Vec<(String, &Value)> = entries
                .iter()
                .map(|(k, v)| (dump_text(k), v))
                .collect();
            rendered.sort_by(|a, b| a.0.cmp(&b.0));
            out.push_str("{\n");
            for (key, val) in rendered {
                let _ = write!(out, "{pad}  {key}: ");
                write_value(out, val, indent + 1);
                out.push('\n');
            }
            let _ = write!(out, "{pad}}}");
        }
        Value::Stream(inner) => {
            out.push_str("stream:");
            write_value(out, inner, indent);
        }
        Value::Instance { class, state } => {
            out.push_str("instance{\n");
            let _ = write!(out, "{pad}  class: ");
            write_value(out, class, indent + 1);
            out.push('\n');
            let _ = write!(out, "{pad}  state: ");
            write_seq(out, state, indent + 1, '[', ']');
            out.push('\n');
            let _ = write!(out, "{pad}}}");
        }
    }
}

/// Shared rendering for byte strings (`Bytes` and `Global`): printable ASCII
/// (and empty) render quoted with `prefix`, anything else renders as hex —
/// matching the original `Bytes`-only behavior (`prefix = "b"` reproduces it
/// exactly), reused for `Global` with `prefix = "global:"`.
fn write_bytes_body(out: &mut String, b: &[u8], prefix: &str) {
    if b.is_empty() {
        let _ = write!(out, "{prefix}\"\"");
    } else if b.iter().all(|c| (0x20..0x7F).contains(c)) {
        let _ = write!(out, "{prefix}\"{}\"", String::from_utf8_lossy(b));
    } else {
        let _ = write!(out, "hex:{}", hex(b));
    }
}

fn write_seq(out: &mut String, items: &[Value], indent: usize, open: char, close: char) {
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
        write_value(out, item, indent + 1);
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
    fn dump_global_and_instance() {
        assert_eq!(
            dump_text(&Value::Global(b"__builtin__.set".to_vec())),
            "global:\"__builtin__.set\""
        );
        let inst = Value::Instance {
            class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
            state: vec![Value::Int(1)],
        };
        assert_eq!(
            dump_text(&inst),
            "instance{\n  class: b\"M.Cls\"\n  state: [\n    1\n  ]\n}"
        );
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
