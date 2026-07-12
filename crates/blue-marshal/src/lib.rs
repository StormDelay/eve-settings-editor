//! Decoder for CCP's "blue marshal" serialization used by EVE Online
//! settings files. Reference: ntt/reverence src/blue/marshal.{h,c}.

pub mod error;
pub mod reader;
pub mod value;

pub use error::{DecodeError, ErrorKind};
pub use value::{dump_text, Value};
