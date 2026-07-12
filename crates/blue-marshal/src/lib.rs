//! Decoder for CCP's "blue marshal" serialization used by EVE Online
//! settings files. Reference: ntt/reverence src/blue/marshal.{h,c}.

pub mod decode;
pub mod error;
pub mod opcodes;
pub mod reader;
pub mod string_table;
pub mod value;

pub use decode::decode;
pub use error::{DecodeError, ErrorKind};
pub use value::{dump_text, Value};
