//! Blue-marshal opcode constants. Values verified against
//! `vendor/reverence/src/blue/marshal.h` (commit 9ded855) in Task 4; see
//! `docs/format-notes.md` for the authoritative table.

pub const NONE: u8 = 0x01;
pub const GLOBAL: u8 = 0x02;
pub const INT64: u8 = 0x03;
pub const INT32: u8 = 0x04;
pub const INT16: u8 = 0x05;
pub const INT8: u8 = 0x06;
pub const MINUSONE: u8 = 0x07;
pub const ZERO: u8 = 0x08;
pub const ONE: u8 = 0x09;
pub const FLOAT: u8 = 0x0A;
pub const FLOAT0: u8 = 0x0B;
pub const STRINGL: u8 = 0x0D;
pub const STRING0: u8 = 0x0E;
pub const STRING1: u8 = 0x0F;
pub const STRING: u8 = 0x10;
pub const STRINGR: u8 = 0x11;
pub const UNICODE: u8 = 0x12;
pub const BUFFER: u8 = 0x13;
pub const TUPLE: u8 = 0x14;
pub const LIST: u8 = 0x15;
pub const DICT: u8 = 0x16;
pub const INSTANCE: u8 = 0x17;
pub const BLUE: u8 = 0x18;
pub const CALLBACK: u8 = 0x19;
pub const REF: u8 = 0x1B;
pub const CHECKSUM: u8 = 0x1C;
pub const TRUE: u8 = 0x1F;
pub const FALSE: u8 = 0x20;
pub const PICKLER: u8 = 0x21;
pub const REDUCE: u8 = 0x22;
pub const NEWOBJ: u8 = 0x23;
pub const TUPLE0: u8 = 0x24;
pub const TUPLE1: u8 = 0x25;
pub const LIST0: u8 = 0x26;
pub const LIST1: u8 = 0x27;
pub const UNICODE0: u8 = 0x28;
pub const UNICODE1: u8 = 0x29;
pub const DBROW: u8 = 0x2A;
pub const STREAM: u8 = 0x2B;
pub const TUPLE2: u8 = 0x2C;
pub const MARK: u8 = 0x2D;
pub const UTF8: u8 = 0x2E;
pub const LONG: u8 = 0x2F;

/// Stream magic byte (`PROTOCOL_ID`, marshal.h:35).
pub const PROTOCOL: u8 = 0x7E;
/// Bit OR-ed into an opcode byte to mark the object as shared (marshal.h:89).
pub const SHARED_FLAG: u8 = 0x40;
