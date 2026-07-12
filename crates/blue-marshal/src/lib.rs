//! Decoder for CCP's "blue marshal" serialization used by EVE Online
//! settings files. Reference: ntt/reverence src/blue/marshal.{h,c}.

pub mod error;
pub mod reader;

pub use error::{DecodeError, ErrorKind};
