use crate::error::{DecodeError, ErrorKind};

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn err(&self, kind: ErrorKind) -> DecodeError {
        DecodeError { offset: self.pos, kind }
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(self.err(ErrorKind::UnexpectedEof));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn read_u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.read_bytes(1)?[0])
    }

    /// Look at the next byte without consuming it. Used where a MARK-vs-object
    /// lookahead decides control flow before dispatch (REDUCE's iterator tail).
    pub fn peek_u8(&self) -> Result<u8, DecodeError> {
        if self.remaining() < 1 {
            return Err(self.err(ErrorKind::UnexpectedEof));
        }
        Ok(self.data[self.pos])
    }

    pub fn read_u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.read_bytes(2)?.try_into().unwrap()))
    }

    pub fn read_u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }

    pub fn read_i64(&mut self) -> Result<i64, DecodeError> {
        Ok(i64::from_le_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    pub fn read_f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_le_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    /// Blue length encoding: one byte, or 0xFF followed by i32 LE (non-negative in practice).
    pub fn read_len(&mut self) -> Result<usize, DecodeError> {
        let b = self.read_u8()?;
        if b == 0xFF {
            Ok(self.read_u32()? as usize)
        } else {
            Ok(b as usize)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn reads_scalars_little_endian() {
        let data = [0x2A, 0x01, 0x02, 0xEF, 0xBE, 0xAD, 0xDE];
        let mut r = Reader::new(&data);
        assert_eq!(r.read_u8().unwrap(), 0x2A);
        assert_eq!(r.read_u16().unwrap(), 0x0201);
        assert_eq!(r.read_u32().unwrap(), 0xDEADBEEF);
        assert_eq!(r.pos(), 7);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn read_len_single_byte_and_extended() {
        let mut r = Reader::new(&[0x0A]);
        assert_eq!(r.read_len().unwrap(), 10);
        // 0xFF escape -> i32 LE follows (signed per marshal.c:99-108, non-negative in practice; observed in real files: 16 FF 76 02 00 00)
        let mut r = Reader::new(&[0xFF, 0x76, 0x02, 0x00, 0x00]);
        assert_eq!(r.read_len().unwrap(), 0x0276);
    }

    #[test]
    fn reads_i64_and_f64_little_endian() {
        let mut r = Reader::new(&[
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // i64 -1
            0, 0, 0, 0, 0, 0, 0x04, 0x40, // f64 2.5
        ]);
        assert_eq!(r.read_i64().unwrap(), -1);
        assert_eq!(r.read_f64().unwrap(), 2.5);
    }

    #[test]
    fn eof_error_carries_offset() {
        let mut r = Reader::new(&[0x01]);
        r.read_u8().unwrap();
        let err = r.read_u32().unwrap_err();
        assert_eq!(err.offset, 1);
        assert_eq!(err.kind, ErrorKind::UnexpectedEof);
    }

    #[test]
    fn read_bytes_slices_without_copy() {
        let data = [1, 2, 3, 4];
        let mut r = Reader::new(&data);
        assert_eq!(r.read_bytes(3).unwrap(), &[1, 2, 3]);
        assert!(r.read_bytes(2).is_err());
    }

    #[test]
    fn peek_u8_does_not_advance() {
        let mut r = Reader::new(&[0x2D, 0x09]);
        assert_eq!(r.peek_u8().unwrap(), 0x2D);
        assert_eq!(r.pos(), 0); // unchanged
        assert_eq!(r.read_u8().unwrap(), 0x2D); // same byte, now consumed
        assert_eq!(r.peek_u8().unwrap(), 0x09);
        r.read_u8().unwrap();
        assert_eq!(r.peek_u8().unwrap_err().kind, ErrorKind::UnexpectedEof);
    }
}
