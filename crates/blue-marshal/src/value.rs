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
        Value::Bytes(b) => {
            if !b.is_empty() && b.iter().all(|c| (0x20..0x7F).contains(c)) {
                let _ = write!(out, "b\"{}\"", String::from_utf8_lossy(b));
            } else if b.is_empty() {
                out.push_str("b\"\"");
            } else {
                let _ = write!(out, "hex:{}", hex(b));
            }
        }
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
