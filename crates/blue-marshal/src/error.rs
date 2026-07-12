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
    UnknownFlags(u8),
    BadRef(usize),
    BadStringRef(usize),
    BadUtf8,
    Unsupported(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "decode error at offset {:#x}: {:?}", self.offset, self.kind)
    }
}

impl std::error::Error for DecodeError {}
