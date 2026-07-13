//! Encoder for a blue-marshal stream — the exact inverse of [`crate::decode`].
//! For any tree produced by `decode`, the output is byte-identical to the
//! original stream (gated over the full corpus in tests/corpus.rs). Every
//! canonical opcode choice below is corpus-proven exact; see
//! `docs/format-notes.md`, "Corpus canonicality measurements (2026-07-13)".

use std::collections::HashSet;

use crate::decode::MAX_DEPTH;
use crate::error::{EncodeError, EncodeErrorKind};
use crate::opcodes as op;
use crate::value::Value;

/// Encode a [`Value`] into a complete blue-marshal stream.
pub fn encode(root: &Value) -> Result<Vec<u8>, EncodeError> {
    encode_at_depth(root, 0)
}

/// Inner entry point threading the nesting depth through embedded STREAM
/// encodes, mirroring `decode_at_depth`.
fn encode_at_depth(root: &Value, depth: usize) -> Result<Vec<u8>, EncodeError> {
    let mut e = Encoder {
        out: vec![op::PROTOCOL, 0, 0, 0, 0], // count patched below
        map: Vec::new(),
        stored: HashSet::new(),
        depth,
    };
    e.emit(root)?;
    let count = e.map.len();
    for &slot in &e.map {
        if slot < 1 || slot as usize > count {
            return Err(EncodeError { kind: EncodeErrorKind::SlotOutOfRange { slot, count } });
        }
    }
    // The reference reads the header count and every map entry as signed i32.
    let count32 =
        i32::try_from(count).map_err(|_| EncodeError { kind: EncodeErrorKind::TooLong(count) })?;
    e.out[1..5].copy_from_slice(&count32.to_le_bytes());
    let map = std::mem::take(&mut e.map);
    for slot in map {
        e.out.extend_from_slice(&(slot as i32).to_le_bytes());
    }
    Ok(e.out)
}

struct Encoder {
    out: Vec<u8>,
    /// Tail-map slot numbers in encounter order (one entry per `Shared`).
    map: Vec<u32>,
    /// Slots whose `Shared` node has fully emitted — the only valid `Ref`
    /// targets, mirroring the decoder's store-on-completion semantics.
    stored: HashSet<u32>,
    depth: usize,
}

impl Encoder {
    fn emit(&mut self, v: &Value) -> Result<(), EncodeError> {
        // Depth guard symmetric with decode's (`load` charges one level per
        // node), so any tree that decoded successfully re-encodes in bound.
        if self.depth >= MAX_DEPTH {
            return Err(EncodeError { kind: EncodeErrorKind::TooDeep });
        }
        self.depth += 1;
        let result = match v {
            Value::Shared { slot, value } => {
                // Map entry claimed at *encounter* (open) time, before the
                // children emit — marshal.c's STORE/RESERVE_SLOT order. The
                // slot becomes REF-able only after the body completes.
                self.map.push(*slot);
                let r = self.emit_body(value, op::SHARED_FLAG);
                if r.is_ok() {
                    self.stored.insert(*slot);
                }
                r
            }
            other => self.emit_body(other, 0),
        };
        self.depth -= 1;
        result
    }

    /// Emit one node's opcode, payload, and children. `flag` is 0 or
    /// SHARED_FLAG; the flag is only legal on nodes whose emitted opcode the
    /// reference actually stores (decode.rs `stores_shared`) — anywhere else
    /// the reference ignores it, the tail map would desynchronize, and we
    /// reject instead.
    fn emit_body(&mut self, v: &Value, flag: u8) -> Result<(), EncodeError> {
        if flag != 0 && !storable_with_flag(v) {
            return Err(EncodeError { kind: EncodeErrorKind::NotStorable(kind_name(v)) });
        }
        match v {
            Value::None => self.out.push(op::NONE),
            Value::Bool(true) => self.out.push(op::TRUE),
            Value::Bool(false) => self.out.push(op::FALSE),
            Value::Int(n) => match *n {
                -1 => self.out.push(op::MINUSONE),
                0 => self.out.push(op::ZERO),
                1 => self.out.push(op::ONE),
                n if i8::try_from(n).is_ok() => {
                    self.out.push(op::INT8);
                    self.out.push(n as i8 as u8);
                }
                n if i16::try_from(n).is_ok() => {
                    self.out.push(op::INT16);
                    self.out.extend_from_slice(&(n as i16).to_le_bytes());
                }
                n if i32::try_from(n).is_ok() => {
                    self.out.push(op::INT32);
                    self.out.extend_from_slice(&(n as i32).to_le_bytes());
                }
                n => {
                    self.out.push(op::INT64);
                    self.out.extend_from_slice(&n.to_le_bytes());
                }
            },
            Value::Long(bytes) => {
                self.out.push(op::LONG | flag);
                self.write_len(bytes.len())?;
                self.out.extend_from_slice(bytes);
            }
            Value::Float(f) => {
                if f.to_bits() == 0f64.to_bits() {
                    self.out.push(op::FLOAT0);
                } else {
                    // Bit-exact via to_le_bytes; −0.0 and NaN payloads pass
                    // through untouched.
                    self.out.push(op::FLOAT);
                    self.out.extend_from_slice(&f.to_le_bytes());
                }
            }
            Value::Bytes(b) => match b.len() {
                0 => self.out.push(op::STRING0),
                1 => {
                    self.out.push(op::STRING1);
                    self.out.push(b[0]);
                }
                n => {
                    self.out.push(op::BUFFER | flag);
                    self.write_len(n)?;
                    self.out.extend_from_slice(b);
                }
            },
            Value::Str(s) => {
                self.out.push(op::UTF8);
                self.write_len(s.len())?;
                self.out.extend_from_slice(s.as_bytes());
            }
            Value::StrUcs2(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                match units.len() {
                    0 => self.out.push(op::UNICODE0),
                    1 => {
                        self.out.push(op::UNICODE1);
                        self.out.extend_from_slice(&units[0].to_le_bytes());
                    }
                    n => {
                        self.out.push(op::UNICODE);
                        self.write_len(n)?;
                        for u in &units {
                            self.out.extend_from_slice(&u.to_le_bytes());
                        }
                    }
                }
            }
            Value::StrTable(idx) => {
                if *idx == 0 {
                    return Err(EncodeError { kind: EncodeErrorKind::BadTableIndex });
                }
                self.out.push(op::STRINGR);
                self.write_len(*idx as usize)?;
            }
            Value::Tuple(items) => {
                match items.len() {
                    0 => self.out.push(op::TUPLE0), // does not store; flag rejected above
                    1 => self.out.push(op::TUPLE1 | flag),
                    2 => self.out.push(op::TUPLE2 | flag),
                    n => {
                        self.out.push(op::TUPLE | flag);
                        self.write_len(n)?;
                    }
                }
                for item in items {
                    self.emit(item)?;
                }
            }
            Value::List(items) => {
                match items.len() {
                    0 => self.out.push(op::LIST0 | flag), // LIST0 stores (the reference asymmetry)
                    1 => self.out.push(op::LIST1 | flag),
                    n => {
                        self.out.push(op::LIST | flag);
                        self.write_len(n)?;
                    }
                }
                for item in items {
                    self.emit(item)?;
                }
            }
            Value::Dict(entries) => {
                self.out.push(op::DICT | flag);
                self.write_len(entries.len())?;
                for (key, value) in entries {
                    self.emit(value)?; // wire order: value first (marshal.c:182)
                    self.emit(key)?;
                }
            }
            Value::Stream(inner) => {
                let bytes = encode_at_depth(inner, self.depth)?;
                self.out.push(op::STREAM | flag);
                self.write_len(bytes.len())?;
                self.out.extend_from_slice(&bytes);
            }
            Value::Global(name) => {
                self.out.push(op::GLOBAL | flag);
                self.write_len(name.len())?;
                self.out.extend_from_slice(name);
            }
            Value::Instance { class, state } => {
                self.out.push(op::INSTANCE | flag);
                self.emit(class)?;
                self.emit(state)?;
            }
            Value::Reduce { ctor, items, pairs } => {
                self.out.push(op::REDUCE | flag);
                self.emit(ctor)?;
                for item in items {
                    self.emit(item)?;
                }
                self.out.push(op::MARK);
                for (key, value) in pairs {
                    self.emit(key)?; // iterated order: key first (marshal.c:182)
                    self.emit(value)?;
                }
                self.out.push(op::MARK);
            }
            Value::Ref(slot) => {
                if !self.stored.contains(slot) {
                    return Err(EncodeError { kind: EncodeErrorKind::RefBeforeStore(*slot) });
                }
                self.out.push(op::REF);
                self.write_len(*slot as usize)?;
            }
            Value::Shared { .. } => {
                // Only reachable as the direct child of another `Shared` (the
                // outer wrapper peels in `emit`); one opcode byte carries one
                // flag, so this cannot exist on the wire. The flag!=0 check
                // above already rejected it (Shared is not storable), so this
                // arm exists for match exhaustiveness on the flag==0 path,
                // which `emit` makes unreachable.
                return Err(EncodeError { kind: EncodeErrorKind::NotStorable("shared") });
            }
        }
        Ok(())
    }

    /// Blue length encoding, always minimal (corpus-proven: zero non-minimal
    /// escapes among 228,549): 0..=254 as one byte, 255.. as 0xFF + i32 LE.
    fn write_len(&mut self, n: usize) -> Result<(), EncodeError> {
        if n < 255 {
            self.out.push(n as u8);
        } else {
            let v = i32::try_from(n)
                .map_err(|_| EncodeError { kind: EncodeErrorKind::TooLong(n) })?;
            self.out.push(0xFF);
            self.out.extend_from_slice(&v.to_le_bytes());
        }
        Ok(())
    }
}

/// Whether SHARED_FLAG may be emitted on this node — true exactly when the
/// opcode `emit_body` will choose is in decode.rs's `stores_shared` set.
/// Length-dependent: `Bytes` of len ≤ 1 emits STRING0/STRING1 and an empty
/// `Tuple` emits TUPLE0, none of which store (while LIST0 does).
fn storable_with_flag(v: &Value) -> bool {
    match v {
        Value::Long(_)
        | Value::List(_)
        | Value::Dict(_)
        | Value::Stream(_)
        | Value::Global(_)
        | Value::Instance { .. }
        | Value::Reduce { .. } => true,
        Value::Bytes(b) => b.len() >= 2,
        Value::Tuple(items) => !items.is_empty(),
        _ => false,
    }
}

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::None => "none",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Long(_) => "long",
        Value::Float(_) => "float",
        Value::Bytes(_) => "bytes",
        Value::Str(_) => "str",
        Value::StrUcs2(_) => "str-ucs2",
        Value::StrTable(_) => "str-table",
        Value::Tuple(_) => "tuple",
        Value::List(_) => "list",
        Value::Dict(_) => "dict",
        Value::Stream(_) => "stream",
        Value::Global(_) => "global",
        Value::Instance { .. } => "instance",
        Value::Reduce { .. } => "reduce",
        Value::Shared { .. } => "shared",
        Value::Ref(_) => "ref",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode;

    // header: magic 0x7E + i32 shared-count 0 (same helper as decode tests)
    fn stream(body: &[u8]) -> Vec<u8> {
        let mut v = vec![0x7E, 0, 0, 0, 0];
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn encodes_scalars_canonically() {
        assert_eq!(encode(&Value::None).unwrap(), stream(&[0x01]));
        assert_eq!(encode(&Value::Bool(true)).unwrap(), stream(&[0x1F]));
        assert_eq!(encode(&Value::Bool(false)).unwrap(), stream(&[0x20]));
        assert_eq!(encode(&Value::Int(-1)).unwrap(), stream(&[0x07]));
        assert_eq!(encode(&Value::Int(0)).unwrap(), stream(&[0x08]));
        assert_eq!(encode(&Value::Int(1)).unwrap(), stream(&[0x09]));
        assert_eq!(encode(&Value::Int(42)).unwrap(), stream(&[0x06, 0x2A]));
        assert_eq!(encode(&Value::Int(-2)).unwrap(), stream(&[0x06, 0xFE]));
        // Width boundaries: 127 is the last INT8, 128 the first INT16, etc.
        assert_eq!(encode(&Value::Int(127)).unwrap(), stream(&[0x06, 0x7F]));
        assert_eq!(encode(&Value::Int(128)).unwrap(), stream(&[0x05, 0x80, 0x00]));
        assert_eq!(encode(&Value::Int(0x1234)).unwrap(), stream(&[0x05, 0x34, 0x12]));
        assert_eq!(
            encode(&Value::Int(0x12345678)).unwrap(),
            stream(&[0x04, 0x78, 0x56, 0x34, 0x12])
        );
        assert_eq!(
            encode(&Value::Int(0x0102030405060708)).unwrap(),
            stream(&[0x03, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01])
        );
        assert_eq!(
            encode(&Value::Float(2.5)).unwrap(),
            stream(&[0x0A, 0, 0, 0, 0, 0, 0, 0x04, 0x40])
        );
        assert_eq!(encode(&Value::Float(0.0)).unwrap(), stream(&[0x0B]));
        // −0.0 has its own bit pattern and must NOT collapse to FLOAT0.
        assert_eq!(
            encode(&Value::Float(-0.0)).unwrap(),
            stream(&[0x0A, 0, 0, 0, 0, 0, 0, 0, 0x80])
        );
        assert_eq!(
            encode(&Value::Long(vec![0x2A])).unwrap(),
            stream(&[0x2F, 0x01, 0x2A])
        );
    }

    #[test]
    fn encodes_string_family_by_tag() {
        assert_eq!(encode(&Value::Bytes(vec![])).unwrap(), stream(&[0x0E]));
        assert_eq!(
            encode(&Value::Bytes(b"x".to_vec())).unwrap(),
            stream(&[0x0F, b'x'])
        );
        assert_eq!(
            encode(&Value::Bytes(b"foo".to_vec())).unwrap(),
            stream(&[0x13, 0x03, b'f', b'o', b'o'])
        );
        assert_eq!(
            encode(&Value::Str("abc".into())).unwrap(),
            stream(&[0x2E, 0x03, b'a', b'b', b'c'])
        );
        // Empty string: the tag picks the opcode (both exist in the corpus).
        assert_eq!(encode(&Value::Str(String::new())).unwrap(), stream(&[0x2E, 0x00]));
        assert_eq!(encode(&Value::StrUcs2(String::new())).unwrap(), stream(&[0x28]));
        assert_eq!(
            encode(&Value::StrUcs2("x".into())).unwrap(),
            stream(&[0x29, b'x', 0x00])
        );
        assert_eq!(
            encode(&Value::StrUcs2("hi".into())).unwrap(),
            stream(&[0x12, 0x02, b'h', 0x00, b'i', 0x00])
        );
        assert_eq!(encode(&Value::StrTable(7)).unwrap(), stream(&[0x11, 0x07]));
        assert_eq!(
            encode(&Value::StrTable(0)).unwrap_err().kind,
            EncodeErrorKind::BadTableIndex
        );
    }

    #[test]
    fn long_lengths_use_the_ff_escape_minimally() {
        // 254 bytes: single-byte count. 255 bytes: 0xFF + i32 LE.
        let b254 = Value::Bytes(vec![b'a'; 254]);
        let e254 = encode(&b254).unwrap();
        assert_eq!(&e254[5..7], &[0x13, 254]);
        let b255 = Value::Bytes(vec![b'a'; 255]);
        let e255 = encode(&b255).unwrap();
        assert_eq!(&e255[5..11], &[0x13, 0xFF, 255, 0, 0, 0]);
        assert_eq!(decode(&e254).unwrap(), b254);
        assert_eq!(decode(&e255).unwrap(), b255);
    }

    #[test]
    fn encodes_containers_and_dict_value_first() {
        assert_eq!(encode(&Value::Tuple(vec![])).unwrap(), stream(&[0x24]));
        assert_eq!(
            encode(&Value::Tuple(vec![Value::None])).unwrap(),
            stream(&[0x25, 0x01])
        );
        assert_eq!(
            encode(&Value::Tuple(vec![Value::Int(0), Value::Int(1)])).unwrap(),
            stream(&[0x2C, 0x08, 0x09])
        );
        assert_eq!(
            encode(&Value::Tuple(vec![Value::None, Value::None, Value::None])).unwrap(),
            stream(&[0x14, 0x03, 0x01, 0x01, 0x01])
        );
        assert_eq!(encode(&Value::List(vec![])).unwrap(), stream(&[0x26]));
        assert_eq!(
            encode(&Value::List(vec![Value::Int(1)])).unwrap(),
            stream(&[0x27, 0x09])
        );
        let d = Value::Dict(vec![(Value::Bytes(b"foo".to_vec()), Value::Int(0))]);
        assert_eq!(
            encode(&d).unwrap(),
            stream(&[0x16, 0x01, 0x08, 0x13, 0x03, b'f', b'o', b'o'])
        );
    }

    #[test]
    fn reencodes_shared_fixtures_byte_identically() {
        // The exact fixture streams from decode.rs's shared tests round-trip
        // decode → encode unchanged — non-identity tail map included.
        let shared_buffer: Vec<u8> = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x53, 0x02, b'h', b'i', // BUFFER|SHARED, len 2
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: slot 1
        ];
        let nonidentity_map: Vec<u8> = vec![
            0x7E, 0x02, 0x00, 0x00, 0x00, // header, shared_count = 2
            0x14, 0x03, // TUPLE, 3 elements
            0x65, // TUPLE1|SHARED -> map[0] = 2
            0x6F, 0x01, 0x2A, // LONG|SHARED, len 1 -> map[1] = 1
            0x1B, 0x01, // REF -> slot 1
            0x1B, 0x02, // REF -> slot 2
            0x02, 0x00, 0x00, 0x00, // tail map: entry 0 = slot 2
            0x01, 0x00, 0x00, 0x00, // tail map: entry 1 = slot 1
        ];
        let shared_instance: Vec<u8> = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00,
            0x2C, // TUPLE2
            0x57, // INSTANCE|SHARED
            0x13, 0x05, b'M', b'.', b'C', b'l', b's', // class BUFFER
            0x09, // state ONE
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00,
        ];
        let shared_reduce: Vec<u8> = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00,
            0x2C, // TUPLE2
            0x62, // REDUCE|SHARED
            0x2C, 0x02, 3, b'M', b'.', b'f', 0x24, // ctor (GLOBAL "M.f", ())
            0x2D, 0x2D, // empty iterator tail
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00,
        ];
        for data in [shared_buffer, nonidentity_map, shared_instance, shared_reduce] {
            let v = decode(&data).unwrap();
            assert_eq!(encode(&v).unwrap(), data);
        }
    }

    #[test]
    fn nested_stream_reencodes_recursively() {
        // STREAM (0x2B) payload is itself a complete stream holding ONE.
        let data = stream(&[0x2B, 0x06, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x09]);
        let v = decode(&data).unwrap();
        assert_eq!(v, Value::Stream(Box::new(Value::Int(1))));
        assert_eq!(encode(&v).unwrap(), data);
    }

    #[test]
    fn encode_rejects_invalid_sharing() {
        // Flag on a node whose opcode does not store.
        let bad = Value::Shared { slot: 1, value: Box::new(Value::Int(5)) };
        assert_eq!(
            encode(&bad).unwrap_err().kind,
            EncodeErrorKind::NotStorable("int")
        );
        // Empty tuple emits TUPLE0, which does not store either.
        let bad_t0 = Value::Shared { slot: 1, value: Box::new(Value::Tuple(vec![])) };
        assert_eq!(
            encode(&bad_t0).unwrap_err().kind,
            EncodeErrorKind::NotStorable("tuple")
        );
        // ...while LIST0 does store (the reference asymmetry).
        let ok_l0 = Value::Tuple(vec![
            Value::Shared { slot: 1, value: Box::new(Value::List(vec![])) },
            Value::Ref(1),
        ]);
        assert_eq!(decode(&encode(&ok_l0).unwrap()).unwrap(), ok_l0);
        // Ref with no Shared anywhere.
        let dangling = Value::Tuple(vec![Value::Ref(1)]);
        assert_eq!(
            encode(&dangling).unwrap_err().kind,
            EncodeErrorKind::RefBeforeStore(1)
        );
        // Slot number beyond the shared count.
        let far = Value::Shared { slot: 3, value: Box::new(Value::List(vec![])) };
        assert_eq!(
            encode(&far).unwrap_err().kind,
            EncodeErrorKind::SlotOutOfRange { slot: 3, count: 1 }
        );
        // A self-referential Ref inside its own Shared subtree is rejected,
        // matching the decoder's store-on-completion semantics.
        let cyclic = Value::Shared {
            slot: 1,
            value: Box::new(Value::List(vec![Value::Ref(1)])),
        };
        assert_eq!(
            encode(&cyclic).unwrap_err().kind,
            EncodeErrorKind::RefBeforeStore(1)
        );
    }

    #[test]
    fn encode_depth_guard_matches_decode() {
        let mut v = Value::Tuple(vec![Value::None]);
        for _ in 0..200 {
            v = Value::Tuple(vec![v]);
        }
        assert_eq!(encode(&v).unwrap_err().kind, EncodeErrorKind::TooDeep);
    }
}
