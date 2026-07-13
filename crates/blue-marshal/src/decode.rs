//! Recursive decoder for a blue-marshal stream. The single entry point is
//! [`decode`]. Wire-format rules follow `docs/format-notes.md`, whose
//! "Per-opcode encoding details (verified against marshal.c)" sections are
//! authoritative; every non-obvious choice below cites the marshal.c line it
//! was verified against.
//!
//! Known deviation: the reference stores containers into their shared slot at
//! container *open* (NEW_SEQUENCE/RESERVE_SLOT), which lets a REF inside a
//! container point back at the container itself — a cyclic reference. This
//! decoder stores the completed value *after* its children decode, so such a
//! self-referential REF fails with `BadRef` instead. An owned `Value` tree
//! cannot represent a cycle anyway, and no corpus file contains one (all 5022
//! decode cleanly).

use crate::error::{DecodeError, ErrorKind};
use crate::opcodes as op;
use crate::reader::Reader;
use crate::string_table::STRING_TABLE;
use crate::value::Value;

/// Maximum object-hierarchy depth, mirroring the reference
/// (`#define MAX_DEPTH 64`, marshal.c:22; enforced by PUSH_CONTAINER,
/// marshal.c:167-171, "object hierarchy too deep"). Without this, a corrupt
/// stream of chained container opcodes recurses one frame per byte and
/// overflows the stack — an uncatchable process abort — instead of returning
/// a `DecodeError`. `pub(crate)` so `value.rs`'s `dump_text` can reuse the
/// same bound for its own nested-stream decode attempts, which call this
/// module's `decode` fresh (at depth 0) and so need their own depth guard.
pub(crate) const MAX_DEPTH: usize = 64;

/// Decode a complete blue-marshal stream into a [`Value`].
pub fn decode(data: &[u8]) -> Result<Value, DecodeError> {
    decode_at_depth(data, 0)
}

/// Inner entry point that threads the nesting depth through embedded STREAM
/// decodes, so a chain of nested streams also stays within [`MAX_DEPTH`].
fn decode_at_depth(data: &[u8], depth: usize) -> Result<Value, DecodeError> {
    // Header: magic (marshal.c:467) then shared-object map size, read as a
    // signed i32 LE (marshal.c:478).
    let mut header = Reader::new(data);
    let magic = header.read_u8()?;
    if magic != op::PROTOCOL {
        return Err(DecodeError { offset: 0, kind: ErrorKind::BadMagic(magic) });
    }
    let shared_count = header.read_u32()? as i32;
    if shared_count < 0 {
        // Non-negative in practice; a negative map size is malformed. No new
        // ErrorKind variant per task constraints.
        return Err(DecodeError { offset: 1, kind: ErrorKind::Unsupported("negative shared count") });
    }
    let shared_count = shared_count as usize;

    // The last `shared_count * 4` bytes are the tail map; the object stream
    // (and every payload read) must stop at its start (marshal.c:489-502).
    // Bound the payload Reader to `data[..end]` so reads cannot spill into the
    // map, while map entries are read separately from the full `data` slice.
    // `end >= 5` also covers marshal.c:482's "room for map" check.
    let map_start = data
        .len()
        .checked_sub(shared_count * 4)
        .filter(|&end| end >= 5)
        .ok_or(DecodeError { offset: 1, kind: ErrorKind::UnexpectedEof })?;

    let mut r = Reader::new(&data[..map_start]);
    r.read_bytes(5)?; // skip the header we already parsed (safe: map_start >= 5)

    let mut dec = Decoder {
        shared: vec![false; shared_count],
        shared_next: 0,
        data,
        depth,
    };
    let value = dec.load(&mut r)?;
    if r.remaining() > 0 {
        return Err(DecodeError {
            offset: r.pos(),
            kind: ErrorKind::TrailingBytes(r.remaining()),
        });
    }
    if dec.shared_next != dec.shared.len() {
        return Err(DecodeError {
            offset: r.pos(),
            kind: ErrorKind::UnconsumedSharedMap {
                declared: dec.shared.len(),
                stored: dec.shared_next,
            },
        });
    }
    Ok(value)
}

struct Decoder<'a> {
    /// Whether each shared slot (indexed by map entry - 1) has been stored.
    shared: Vec<bool>,
    /// Encounter counter: index of the next tail-map entry to consume.
    shared_next: usize,
    /// Full stream, used only to read tail-map slot numbers.
    data: &'a [u8],
    /// Current object-nesting depth; guarded against [`MAX_DEPTH`].
    depth: usize,
}

/// Opcodes for which the reference actually stores a shared object when
/// SHARED_FLAG is set (CHECK_SHARED / NEW_SEQUENCE call sites: LONG @602,
/// STRINGL/BUFFER/STREAM @667, LIST0 @714, DICT @733/739, GLOBAL @782, and the
/// TUPLE/LIST containers via NEW_SEQUENCE @212). Note the asymmetry: LIST0 is
/// eligible but TUPLE0/STRING0/UNICODE0 are served from the constants table
/// (marshal.c:1246 etc.) and are NOT eligible. On every other opcode the flag
/// is silently ignored, so we must not consume a map slot for it.
/// INSTANCE and REDUCE reserve their slot at container open via a different
/// mechanism (RESERVE_SLOT, marshal.c:128-139) rather than CHECK_SHARED, but
/// it lands in the same `shared_count`/`shared_map` sequence at the same
/// encounter-order position, so folding them into this same "reserve before
/// children, store after" path (see `load` below) reproduces it exactly.
/// DBROW/NEWOBJ are also eligible in the reference but are Unsupported in
/// this task (absent from the corpus), so they error before any store
/// matters.
fn stores_shared(code: u8) -> bool {
    matches!(
        code,
        op::LONG
            | op::STRINGL
            | op::BUFFER
            | op::STREAM
            | op::LIST0
            | op::DICT
            | op::TUPLE
            | op::TUPLE1
            | op::TUPLE2
            | op::LIST
            | op::LIST1
            | op::GLOBAL
            | op::INSTANCE
            | op::REDUCE
    )
}

impl<'a> Decoder<'a> {
    /// Consume the next tail-map entry (marshal.c STORE/RESERVE_SLOT, 111-139)
    /// and return the 1-based slot it designates. Called at the object's
    /// *encounter* (container-open) time, before its children decode, so
    /// nested shared objects take later map entries than their parent.
    fn reserve_slot(&mut self, at: usize) -> Result<usize, DecodeError> {
        let n = self.shared.len();
        if self.shared_next >= n {
            // More shared-flagged objects than the map has room for
            // (marshal.c:113/131 overflow).
            return Err(DecodeError { offset: at, kind: ErrorKind::BadRef(self.shared_next + 1) });
        }
        let start = self.data.len() - n * 4 + self.shared_next * 4;
        let slot = i32::from_le_bytes(self.data[start..start + 4].try_into().unwrap());
        self.shared_next += 1;
        // Map entries must be 1..=shared_mapsize (marshal.c:492-499);
        // validated here lazily per-reservation rather than upfront.
        if slot < 1 || slot as usize > n {
            return Err(DecodeError { offset: at, kind: ErrorKind::BadRef(slot as usize) });
        }
        Ok(slot as usize)
    }

    fn load(&mut self, r: &mut Reader<'a>) -> Result<Value, DecodeError> {
        let raw = r.read_u8()?;
        let opcode_offset = r.pos() - 1;
        // Depth guard (reference: MAX_DEPTH check in PUSH_CONTAINER,
        // marshal.c:167-171). `load` recurses once per nesting level, so
        // bounding it bounds stack use. No decrement needed on error paths:
        // any error aborts the whole decode.
        if self.depth >= MAX_DEPTH {
            return Err(DecodeError {
                offset: opcode_offset,
                kind: ErrorKind::Unsupported("object hierarchy too deep"),
            });
        }
        self.depth += 1;
        // Reference masks only SHARED_FLAG (0x40) before dispatch
        // (marshal.c:525-526); a set 0x80 bit stays in `code` and falls out as
        // an UnknownOpcode below.
        let shared = raw & op::SHARED_FLAG != 0;
        let code = raw & !op::SHARED_FLAG;

        // Reserve the shared slot BEFORE decoding payload/children, but only on
        // opcodes the reference actually stores — matching its encounter-order
        // slot assignment. The completed value is written back afterwards.
        let slot = if shared && stores_shared(code) {
            Some(self.reserve_slot(opcode_offset)?)
        } else {
            None
        };

        let value = self.load_op(code, r, opcode_offset)?;

        self.depth -= 1;
        Ok(match slot {
            Some(slot) => {
                self.shared[slot - 1] = true;
                Value::Shared { slot: slot as u32, value: Box::new(value) }
            }
            None => value,
        })
    }

    fn load_op(&mut self, code: u8, r: &mut Reader<'a>, at: usize) -> Result<Value, DecodeError> {
        Ok(match code {
            op::NONE => Value::None,
            op::TRUE => Value::Bool(true),
            op::FALSE => Value::Bool(false),
            op::MINUSONE => Value::Int(-1),
            op::ZERO => Value::Int(0),
            op::ONE => Value::Int(1),
            op::INT8 => Value::Int(r.read_u8()? as i8 as i64),
            op::INT16 => Value::Int(r.read_u16()? as i16 as i64),
            op::INT32 => Value::Int(r.read_u32()? as i32 as i64),
            op::INT64 => Value::Int(r.read_i64()?),
            op::FLOAT => Value::Float(r.read_f64()?),
            op::FLOAT0 => Value::Float(0.0),
            // LONG (marshal.c:592-604): READ_LENGTH byte count of a
            // little-endian two's-complement integer; raw bytes kept as-is.
            op::LONG => {
                let n = r.read_len()?;
                Value::Long(r.read_bytes(n)?.to_vec())
            }
            op::STRING0 => Value::Bytes(vec![]),
            op::STRING1 => Value::Bytes(r.read_bytes(1)?.to_vec()),
            // STRING (0x10, marshal.c:643-650) is NOT in `needlength`; it reads
            // its own bare one-byte count with no 0xFF escape.
            op::STRING => {
                let n = r.read_u8()? as usize;
                Value::Bytes(r.read_bytes(n)?.to_vec())
            }
            // STRINGL/BUFFER share one body (marshal.c:658-668): READ_LENGTH
            // raw bytes. (STREAM shares it too but we recurse — see below.)
            op::STRINGL | op::BUFFER => {
                let n = r.read_len()?;
                Value::Bytes(r.read_bytes(n)?.to_vec())
            }
            // STRINGR (marshal.c:630-641): count is a DIRECT index into the
            // fixed table; index 0 and >= table size are rejected. Our
            // STRING_TABLE has index 0 = "" placeholder so wire indices map
            // straight through.
            op::STRINGR => {
                let idx = r.read_len()?;
                if idx < 1 || idx >= STRING_TABLE.len() {
                    return Err(DecodeError { offset: at, kind: ErrorKind::BadStringRef(idx) });
                }
                Value::StrTable(idx as u8)
            }
            op::UNICODE0 => Value::StrUcs2(String::new()),
            // UNICODE1 = exactly one UTF-16LE unit (2 bytes, no count);
            // UNICODE = count code units, payload count*2 bytes (marshal.c:670-688).
            op::UNICODE1 | op::UNICODE => {
                let units = if code == op::UNICODE1 { 1 } else { r.read_len()? };
                let bytes = r.read_bytes(units * 2)?;
                let u16s: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16(&u16s)
                    .map(Value::StrUcs2)
                    .map_err(|_| DecodeError { offset: at, kind: ErrorKind::BadUtf8 })?
            }
            // UTF8 (marshal.c:690-694): READ_LENGTH byte count of UTF-8 bytes.
            op::UTF8 => {
                let n = r.read_len()?;
                let bytes = r.read_bytes(n)?;
                std::str::from_utf8(bytes)
                    .map(|s| Value::Str(s.to_string()))
                    .map_err(|_| DecodeError { offset: at, kind: ErrorKind::BadUtf8 })?
            }
            op::TUPLE0 => Value::Tuple(vec![]),
            op::TUPLE1 => Value::Tuple(vec![self.load(r)?]),
            op::TUPLE2 => {
                let a = self.load(r)?;
                let b = self.load(r)?;
                Value::Tuple(vec![a, b])
            }
            op::TUPLE => {
                let n = r.read_len()?;
                let mut items = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    items.push(self.load(r)?);
                }
                Value::Tuple(items)
            }
            op::LIST0 => Value::List(vec![]),
            op::LIST1 => Value::List(vec![self.load(r)?]),
            op::LIST => {
                let n = r.read_len()?;
                let mut items = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    items.push(self.load(r)?);
                }
                Value::List(items)
            }
            // DICT (marshal.c:725-741 + POPULATE_DICT 182): count entries, each
            // encoded value-first then key; normalized to (key, value).
            op::DICT => {
                let n = r.read_len()?;
                let mut entries = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let value = self.load(r)?; // wire order: value first
                    let key = self.load(r)?;
                    entries.push((key, value));
                }
                Value::Dict(entries)
            }
            // REF (marshal.c:748-764): 1-based index into the shared table;
            // out of range or not-yet-populated -> BadRef.
            op::REF => {
                let idx = r.read_len()?;
                if idx < 1 || idx > self.shared.len() || !self.shared[idx - 1] {
                    return Err(DecodeError { offset: at, kind: ErrorKind::BadRef(idx) });
                }
                Value::Ref(idx as u32)
            }
            // STREAM (marshal.c:658-668): READ_LENGTH raw bytes that are
            // themselves a complete marshal stream; decode recursively,
            // carrying the current depth so nested streams cannot bypass the
            // MAX_DEPTH guard. (Inner errors report offsets relative to the
            // embedded stream.)
            op::STREAM => {
                let n = r.read_len()?;
                let bytes = r.read_bytes(n)?;
                Value::Stream(Box::new(decode_at_depth(bytes, self.depth)?))
            }
            // GLOBAL (marshal.c:766-784): READ_LENGTH byte count of a dotted
            // Python name (e.g. "__builtin__.set"), used elsewhere as a class
            // or callable reference. The reference resolves it via
            // find_global; we have no Python runtime to resolve into, so we
            // keep the raw name. Honors SHARED_FLAG (CHECK_SHARED @782).
            op::GLOBAL => {
                let n = r.read_len()?;
                Value::Global(r.read_bytes(n)?.to_vec())
            }
            // INSTANCE (marshal.c:787-793 open; 913-944 fill): no immediate
            // payload beyond the opcode byte; exactly two plain objects
            // follow in order — the class name, then the state object
            // (applied via __setstate__/__dict__.update in the reference).
            // Both are ordinary objects decoded through the normal recursive
            // path, so SHARED_FLAG on either (e.g. a cached class-name
            // BUFFER) is handled automatically by `load`.
            op::INSTANCE => {
                let class = self.load(r)?;
                let state = self.load(r)?;
                Value::Instance { class: Box::new(class), state: Box::new(state) }
            }
            // REDUCE (marshal.c:789-793 open; 985-1010 fill): one plain
            // object follows — a Tuple shaped (callable, args[, state]) —
            // then an *unconditional* list-then-dict iterator tail: zero or
            // more objects (would be `list.append`ed), a MARK, zero or more
            // (key, value) pairs in that order (POPULATE_DICT's "iterated
            // (key,val)" form, line 182 — note this is the opposite wire
            // order from plain DICT's counted (val,key) form), then a second
            // MARK. Every REDUCE observed in the corpus has an empty tail
            // (MARK immediately follows the ctor tuple twice), but the loop
            // below handles a non-empty one losslessly: appended objects land
            // in `items`, and each (key, value) pair lands in `pairs`, both
            // in wire order, alongside the ctor tuple itself (`ctor`).
            op::REDUCE => {
                let ctor = self.load(r)?;
                let mut items = Vec::new();
                loop {
                    let next = r.peek_u8()?;
                    if next & !op::SHARED_FLAG == op::MARK {
                        r.read_u8()?;
                        break;
                    }
                    items.push(self.load(r)?);
                }
                let mut pairs = Vec::new();
                loop {
                    let next = r.peek_u8()?;
                    if next & !op::SHARED_FLAG == op::MARK {
                        r.read_u8()?;
                        break;
                    }
                    let key = self.load(r)?;
                    let value = self.load(r)?;
                    pairs.push((key, value));
                }
                Value::Reduce { ctor: Box::new(ctor), items, pairs }
            }
            // Complex / deferred types (see docs/format-notes.md); not
            // exercised by any file in the corpus. BLUE/CALLBACK/PICKLER
            // have no case in the reference either.
            op::BLUE => return Err(unsupported(at, "BLUE")),
            op::CALLBACK => return Err(unsupported(at, "CALLBACK")),
            op::CHECKSUM => return Err(unsupported(at, "CHECKSUM")),
            op::PICKLER => return Err(unsupported(at, "PICKLER")),
            op::NEWOBJ => return Err(unsupported(at, "NEWOBJ")),
            op::DBROW => return Err(unsupported(at, "DBROW")),
            op::MARK => return Err(unsupported(at, "MARK")),
            other => {
                return Err(DecodeError { offset: at, kind: ErrorKind::UnknownOpcode(other) })
            }
        })
    }
}

fn unsupported(at: usize, name: &'static str) -> DecodeError {
    DecodeError { offset: at, kind: ErrorKind::Unsupported(name) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    // header: magic 0x7E + u32 shared-count 0
    fn stream(body: &[u8]) -> Vec<u8> {
        let mut v = vec![0x7E, 0, 0, 0, 0];
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn rejects_bad_magic() {
        let err = decode(&[0x00, 0, 0, 0, 0, 0x01]).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::BadMagic(0x00));
    }

    #[test]
    fn decodes_scalar_opcodes() {
        assert_eq!(decode(&stream(&[0x01])).unwrap(), Value::None);
        assert_eq!(decode(&stream(&[0x1F])).unwrap(), Value::Bool(true));
        assert_eq!(decode(&stream(&[0x20])).unwrap(), Value::Bool(false));
        assert_eq!(decode(&stream(&[0x07])).unwrap(), Value::Int(-1));
        assert_eq!(decode(&stream(&[0x08])).unwrap(), Value::Int(0));
        assert_eq!(decode(&stream(&[0x09])).unwrap(), Value::Int(1));
        assert_eq!(decode(&stream(&[0x06, 0x2A])).unwrap(), Value::Int(42));
        assert_eq!(decode(&stream(&[0x05, 0x34, 0x12])).unwrap(), Value::Int(0x1234));
        assert_eq!(
            decode(&stream(&[0x04, 0x78, 0x56, 0x34, 0x12])).unwrap(),
            Value::Int(0x12345678)
        );
        assert_eq!(
            decode(&stream(&[0x0A, 0, 0, 0, 0, 0, 0, 0x04, 0x40])).unwrap(),
            Value::Float(2.5)
        );
        assert_eq!(decode(&stream(&[0x0B])).unwrap(), Value::Float(0.0));
    }

    #[test]
    fn decodes_buffer_and_short_strings() {
        // BUFFER 0x13, len 3, "foo" — observed encoding in real files
        assert_eq!(
            decode(&stream(&[0x13, 0x03, b'f', b'o', b'o'])).unwrap(),
            Value::Bytes(b"foo".to_vec())
        );
        assert_eq!(decode(&stream(&[0x0E])).unwrap(), Value::Bytes(vec![])); // STRING0
        assert_eq!(
            decode(&stream(&[0x0F, b'x'])).unwrap(),
            Value::Bytes(b"x".to_vec())
        ); // STRING1
    }

    #[test]
    fn decodes_containers_dict_wire_order_value_then_key() {
        // TUPLE2(ZERO, ONE)
        assert_eq!(
            decode(&stream(&[0x2C, 0x08, 0x09])).unwrap(),
            Value::Tuple(vec![Value::Int(0), Value::Int(1)])
        );
        // TUPLE0 / TUPLE1 / LIST0 / LIST1
        assert_eq!(decode(&stream(&[0x24])).unwrap(), Value::Tuple(vec![]));
        assert_eq!(
            decode(&stream(&[0x25, 0x01])).unwrap(),
            Value::Tuple(vec![Value::None])
        );
        assert_eq!(decode(&stream(&[0x26])).unwrap(), Value::List(vec![]));
        assert_eq!(
            decode(&stream(&[0x27, 0x09])).unwrap(),
            Value::List(vec![Value::Int(1)])
        );
        // DICT len 1: value ZERO, then key BUFFER "foo" (wire order observed
        // in real core_char files) -> normalized to (key, value)
        let d = decode(&stream(&[0x16, 0x01, 0x08, 0x13, 0x03, b'f', b'o', b'o'])).unwrap();
        assert_eq!(
            d,
            Value::Dict(vec![(Value::Bytes(b"foo".to_vec()), Value::Int(0))])
        );
    }

    #[test]
    fn unknown_opcode_reports_offset() {
        let err = decode(&stream(&[0x3D])).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::UnknownOpcode(0x3D));
        assert_eq!(err.offset, 5); // right after the 5-byte header
    }

    // --- Shared-object mechanics (author additions) ---
    // These hand-build a stream with shared_count = 1 and a tail map of one
    // i32 LE slot number. A SHARED_FLAG-ed object is stored, then REF'd back.

    #[test]
    fn shared_buffer_stored_then_ref() {
        // 7E | count=1 | TUPLE2( BUFFER|SHARED "hi", REF 1 ) | map[0]=1
        //   header:  7E 01 00 00 00
        //   body:    2C            TUPLE2
        //            53 02 68 69   BUFFER|0x40, len 2, "hi"  (stored in slot 1)
        //            1B 01         REF index 1
        //   tail:    01 00 00 00   shared_map[0] = slot 1
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x53, 0x02, b'h', b'i', // BUFFER + SHARED_FLAG, len 2
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: entry 0 = slot 1
        ];
        assert_eq!(
            decode(&data).unwrap(),
            Value::Tuple(vec![
                Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"hi".to_vec())) },
                Value::Ref(1),
            ])
        );
    }

    #[test]
    fn shared_dict_stored_then_ref() {
        // 7E | count=1 | TUPLE2( DICT|SHARED {k:1}, REF 1 ) | map[0]=1
        //   body:    2C                 TUPLE2
        //            56 01              DICT|0x40, 1 entry (stored in slot 1)
        //            09                 value ONE (=1)   [wire order: value first]
        //            13 01 6B           key BUFFER "k"
        //            1B 01              REF index 1
        //   tail:    01 00 00 00        shared_map[0] = slot 1
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x56, 0x01, 0x09, 0x13, 0x01, b'k', // DICT+SHARED, 1 entry (val ONE, key "k")
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: entry 0 = slot 1
        ];
        let dict = Value::Dict(vec![(Value::Bytes(b"k".to_vec()), Value::Int(1))]);
        assert_eq!(
            decode(&data).unwrap(),
            Value::Tuple(vec![
                Value::Shared { slot: 1, value: Box::new(dict) },
                Value::Ref(1),
            ])
        );
    }

    #[test]
    fn ref_to_unpopulated_slot_is_bad_ref() {
        // 7E | count=1 | REF 1 | map[0]=1  — nothing ever stored into slot 1
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x1B, 0x01, // REF -> slot 1 (never populated)
            0x01, 0x00, 0x00, 0x00, // tail map
        ];
        let err = decode(&data).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::BadRef(1));
    }

    #[test]
    fn shared_slots_assigned_in_encounter_order_not_completion_order() {
        // Two shared objects, one nested inside the other, with a NON-identity
        // tail map [2, 1]. The reference consumes map entries at *encounter*
        // (container-open) time, so the outer TUPLE1 takes map[0]=2 (slot 2)
        // and the inner LONG takes map[1]=1 (slot 1). A completion-order
        // implementation would hand them out the other way around (the inner
        // LONG completes first) and flip both REF results below.
        //
        //   header:  7E 02 00 00 00     shared_count = 2
        //   body:    14 03              TUPLE, 3 elements
        //            65                 TUPLE1|SHARED  -> reserves map[0]=2
        //            6F 01 2A           LONG|SHARED, 1 byte 0x2A -> map[1]=1
        //            1B 01              REF 1 -> slot 1 = the LONG
        //            1B 02              REF 2 -> slot 2 = the TUPLE1
        //   tail:    02 00 00 00        map[0] = slot 2
        //            01 00 00 00        map[1] = slot 1
        let data = [
            0x7E, 0x02, 0x00, 0x00, 0x00, // header, shared_count = 2
            0x14, 0x03, // TUPLE, 3 elements
            0x65, // TUPLE1 + SHARED_FLAG
            0x6F, 0x01, 0x2A, // LONG + SHARED_FLAG, len 1, payload 0x2A
            0x1B, 0x01, // REF -> slot 1
            0x1B, 0x02, // REF -> slot 2
            0x02, 0x00, 0x00, 0x00, // tail map: entry 0 = slot 2
            0x01, 0x00, 0x00, 0x00, // tail map: entry 1 = slot 1
        ];
        let long = Value::Shared { slot: 1, value: Box::new(Value::Long(vec![0x2A])) };
        let inner = Value::Shared {
            slot: 2,
            value: Box::new(Value::Tuple(vec![long])),
        };
        assert_eq!(
            decode(&data).unwrap(),
            Value::Tuple(vec![inner, Value::Ref(1), Value::Ref(2)])
        );
    }

    #[test]
    fn deep_nesting_errors_instead_of_overflowing_stack() {
        // A corrupt stream of chained TUPLE1 opcodes recurses one frame per
        // byte; without a depth guard that is an uncatchable stack-overflow
        // abort, not a DecodeError. Reference limit: MAX_DEPTH = 64
        // (marshal.c:22), "object hierarchy too deep" (marshal.c:167-171).
        let err = decode(&stream(&[0x25; 100_000])).unwrap_err();
        assert_eq!(
            err.kind,
            crate::ErrorKind::Unsupported("object hierarchy too deep")
        );
    }

    #[test]
    fn decodes_global_dotted_name() {
        // GLOBAL 0x02, len 15, "__builtin__.set" — dotted name observed in
        // real files (a Python builtin, not personal data).
        let mut body = vec![0x02, 15];
        body.extend_from_slice(b"__builtin__.set");
        assert_eq!(
            decode(&stream(&body)).unwrap(),
            Value::Global(b"__builtin__.set".to_vec())
        );
    }

    #[test]
    fn decodes_instance_class_then_state() {
        // INSTANCE 0x17: two plain child objects follow in order — class
        // name (BUFFER "M.Cls"), then state (here a scalar for simplicity;
        // real files use a DICT, itself already covered elsewhere).
        let data = [
            0x17, // INSTANCE
            0x13, 0x05, b'M', b'.', b'C', b'l', b's', // BUFFER class name
            0x09, // state: ONE
        ];
        assert_eq!(
            decode(&stream(&data)).unwrap(),
            Value::Instance {
                class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
                state: Box::new(Value::Int(1)),
            }
        );
    }

    #[test]
    fn decodes_reduce_ctor_then_double_mark_tail() {
        // REDUCE 0x22: one child object (the (callable, args) tuple), then
        // an always-present list-then-dict iterator tail terminated by two
        // MARK bytes — empty in every real occurrence observed so far.
        let mut data = vec![0x22, 0x2C]; // REDUCE, TUPLE2
        data.push(0x02);
        data.push(3);
        data.extend_from_slice(b"M.f"); // GLOBAL callable "M.f"
        data.push(0x25); // TUPLE1 (args)
        data.push(0x26); // LIST0 (args[0] = [])
        data.push(0x2D); // MARK: end of (empty) list-items phase
        data.push(0x2D); // MARK: end of (empty) dict-items phase
        let ctor = Value::Tuple(vec![
            Value::Global(b"M.f".to_vec()),
            Value::Tuple(vec![Value::List(vec![])]),
        ]);
        assert_eq!(
            decode(&stream(&data)).unwrap(),
            Value::Reduce { ctor: Box::new(ctor), items: vec![], pairs: vec![] }
        );
    }

    #[test]
    fn decodes_reduce_with_nonempty_iterator_tail() {
        // Same framing as above, but exercises the general MARK-terminated
        // loop with real (non-empty) list-items and dict-items, per
        // marshal.c's unconditional LIST_ITERATOR/DICT_ITERATOR switch —
        // never observed in the corpus, but part of the wire spec.
        let mut data = vec![0x22, 0x2C]; // REDUCE, TUPLE2
        data.push(0x02);
        data.push(3);
        data.extend_from_slice(b"M.f");
        data.push(0x24); // TUPLE0 (args = ())
        data.push(0x09); // list-item: ONE
        data.push(0x2D); // MARK: end of list-items phase
        data.push(0x08); // dict-item key: ZERO
        data.push(0x09); // dict-item value: ONE
        data.push(0x2D); // MARK: end of dict-items phase
        let ctor = Value::Tuple(vec![Value::Global(b"M.f".to_vec()), Value::Tuple(vec![])]);
        assert_eq!(
            decode(&stream(&data)).unwrap(),
            Value::Reduce {
                ctor: Box::new(ctor),
                items: vec![Value::Int(1)],
                pairs: vec![(Value::Int(0), Value::Int(1))],
            }
        );
    }

    #[test]
    fn string_0x10_reads_bare_count_no_ff_escape() {
        // Common path: STRING 0x10, count 3, "bar".
        assert_eq!(
            decode(&stream(&[0x10, 0x03, b'b', b'a', b'r'])).unwrap(),
            Value::Bytes(b"bar".to_vec())
        );
        // Count byte 0xFF is a LITERAL 255, NOT a 0xFF->i32 length escape.
        // A read_len-based (buggy) STRING would read the next 4 payload bytes
        // as a u32 length (~2e9) and fail; the correct bare-count reads 255.
        let mut body = vec![0x10, 0xFF];
        body.extend_from_slice(&[b'z'; 255]);
        assert_eq!(
            decode(&stream(&body)).unwrap(),
            Value::Bytes(vec![b'z'; 255])
        );
    }

    #[test]
    fn shared_global_stored_then_ref() {
        // 7E | count=1 | TUPLE2( GLOBAL|SHARED "__builtin__.set", REF 1 ) | map[0]=1
        let mut data = vec![
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x42, 15, // GLOBAL(0x02)|SHARED_FLAG, len 15
        ];
        data.extend_from_slice(b"__builtin__.set");
        data.extend_from_slice(&[0x1B, 0x01]); // REF -> slot 1
        data.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // tail map: slot 1
        let g = Value::Shared {
            slot: 1,
            value: Box::new(Value::Global(b"__builtin__.set".to_vec())),
        };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![g, Value::Ref(1)]));
    }

    #[test]
    fn shared_instance_stored_then_ref() {
        // 7E | count=1 | TUPLE2( INSTANCE|SHARED(class "M.Cls", state ONE), REF 1 ) | map[0]=1
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x57, // INSTANCE(0x17)|SHARED_FLAG — reserves slot 1 at open
            0x13, 0x05, b'M', b'.', b'C', b'l', b's', // class BUFFER "M.Cls"
            0x09, // state: ONE
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: slot 1
        ];
        let inst = Value::Shared {
            slot: 1,
            value: Box::new(Value::Instance {
                class: Box::new(Value::Bytes(b"M.Cls".to_vec())),
                state: Box::new(Value::Int(1)),
            }),
        };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![inst, Value::Ref(1)]));
    }

    #[test]
    fn shared_reduce_stored_then_ref() {
        // 7E | count=1 | TUPLE2( REDUCE|SHARED((M.f, ()), empty tail), REF 1 ) | map[0]=1
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x2C, // TUPLE2
            0x62, // REDUCE(0x22)|SHARED_FLAG — reserves slot 1 at open
            0x2C, // ctor TUPLE2
            0x02, 3, b'M', b'.', b'f', // GLOBAL "M.f"
            0x24, // args TUPLE0
            0x2D, 0x2D, // empty list-then-dict iterator tail
            0x1B, 0x01, // REF -> slot 1
            0x01, 0x00, 0x00, 0x00, // tail map: slot 1
        ];
        let ctor = Value::Tuple(vec![Value::Global(b"M.f".to_vec()), Value::Tuple(vec![])]);
        let red = Value::Shared {
            slot: 1,
            value: Box::new(Value::Reduce { ctor: Box::new(ctor), items: vec![], pairs: vec![] }),
        };
        assert_eq!(decode(&data).unwrap(), Value::Tuple(vec![red, Value::Ref(1)]));
    }

    #[test]
    fn decodes_text_variants_distinctly() {
        // UTF8 "abc"
        assert_eq!(
            decode(&stream(&[0x2E, 0x03, b'a', b'b', b'c'])).unwrap(),
            Value::Str("abc".into())
        );
        // UNICODE0 / UNICODE1 'x' / UNICODE "hi" (2 UTF-16LE units)
        assert_eq!(decode(&stream(&[0x28])).unwrap(), Value::StrUcs2(String::new()));
        assert_eq!(
            decode(&stream(&[0x29, b'x', 0x00])).unwrap(),
            Value::StrUcs2("x".into())
        );
        assert_eq!(
            decode(&stream(&[0x12, 0x02, b'h', 0x00, b'i', 0x00])).unwrap(),
            Value::StrUcs2("hi".into())
        );
        // STRINGR keeps the index, not the content
        assert_eq!(decode(&stream(&[0x11, 0x07])).unwrap(), Value::StrTable(7));
        // UTF8 "" and UNICODE0 stay distinguishable — the corpus contains both
        assert_ne!(
            decode(&stream(&[0x2E, 0x00])).unwrap(),
            decode(&stream(&[0x28])).unwrap()
        );
    }

    #[test]
    fn trailing_bytes_after_root_are_a_hard_error() {
        // Valid ONE root followed by a stray NONE byte before the (empty)
        // tail map — previously silently ignored, now a decode error.
        let err = decode(&stream(&[0x09, 0x01])).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::TrailingBytes(1));
        assert_eq!(err.offset, 6); // header(5) + the ONE opcode
    }

    #[test]
    fn unconsumed_shared_map_is_a_hard_error() {
        // Header declares one shared slot, but no SHARED-flagged object ever
        // occurs: without this check the tree decodes to zero Shared nodes
        // and re-encodes with count 0 — silently different bytes.
        let data = [
            0x7E, 0x01, 0x00, 0x00, 0x00, // header, shared_count = 1
            0x09, // ONE (unflagged)
            0x01, 0x00, 0x00, 0x00, // tail map: slot 1 (never consumed)
        ];
        let err = decode(&data).unwrap_err();
        assert_eq!(
            err.kind,
            crate::ErrorKind::UnconsumedSharedMap { declared: 1, stored: 0 }
        );
    }
}
