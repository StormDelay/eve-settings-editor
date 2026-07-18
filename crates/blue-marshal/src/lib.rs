//! Decoder and encoder for CCP's "blue marshal" serialization used by EVE Online
//! settings files. Reference: ntt/reverence src/blue/marshal.{h,c}.

pub mod decode;
pub mod encode;
pub mod error;
pub mod opcodes;
pub mod reader;
pub mod reshare;
pub mod string_table;
pub mod value;

pub use decode::decode;
pub use encode::encode;
pub use error::{DecodeError, EncodeError, EncodeErrorKind, ErrorKind};
pub use reshare::{inline, reshare};
pub use value::{dump_text, Value};
