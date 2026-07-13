use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// Byte offset in the input where decoding failed.
    pub offset: usize,
    pub kind: ErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    BadMagic(u8),
    UnexpectedEof,
    UnknownOpcode(u8),
    BadRef(usize),
    BadStringRef(usize),
    BadUtf8,
    Unsupported(&'static str),
    /// Bytes remained between the end of the root object and the tail map.
    /// Corpus-proven never to happen (slack_streams = 0 over 5022 files), so
    /// it is a hard error: it would mean we mis-parsed the stream.
    TrailingBytes(usize),
    /// The header declared more shared-map entries than SHARED-flagged
    /// objects actually occurred in the stream (`declared` vs `stored`).
    /// Corpus-proven never to happen (such a file would fail the
    /// byte-identity gate); accepting it would decode to a tree that
    /// re-encodes with a smaller map — a silent divergence.
    UnconsumedSharedMap { declared: usize, stored: usize },
    /// A tail-map slot number appeared more than once. The reference decoder
    /// tolerates this (last store wins), but it makes REF targets ambiguous;
    /// corpus-proven never to happen (0 duplicates across 4,986 files with
    /// shared maps), so it is rejected to keep `Ref(slot)` a unique
    /// identifier for the editing layer.
    DuplicateSharedSlot(usize),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "decode error at offset {:#x}: {:?}", self.offset, self.kind)
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeError {
    pub kind: EncodeErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeErrorKind {
    /// Object hierarchy deeper than MAX_DEPTH (mirrors the decode guard, so
    /// any tree that decoded successfully re-encodes within the bound).
    TooDeep,
    /// A length, count, or shared-map size exceeds the wire format's i32 range.
    TooLong(usize),
    /// SHARED_FLAG requested (via `Value::Shared`) on a node whose emitted
    /// opcode the reference decoder ignores the flag for — emitting it would
    /// desynchronize the tail map. Carries the node's kind name.
    NotStorable(&'static str),
    /// A tail-map slot number outside 1..=shared_count (e.g. after deleting
    /// a `Shared` node while keeping higher slot numbers).
    SlotOutOfRange { slot: u32, count: usize },
    /// A `Ref` appeared before the `Shared` node storing its slot completed —
    /// includes self-referential (cyclic) refs, which this codec rejects on
    /// both sides.
    RefBeforeStore(u32),
    /// `StrTable(0)` — wire index 0 is rejected by the reference decoder.
    BadTableIndex,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "encode error: {:?}", self.kind)
    }
}

impl std::error::Error for EncodeError {}
