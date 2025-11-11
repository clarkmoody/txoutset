use bitcoin::consensus::{Decodable, Encodable, ReadExt};

/// Compact Size
///
/// ```text
/// size <  253        -- 1 byte
/// size <= u16::MAX   -- 3 bytes  (253 + 2 bytes)
/// size <= u32::MAX   -- 5 bytes  (254 + 4 bytes)
/// size >  u32::MAX   -- 9 bytes  (255 + 8 bytes)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompactSize(u64);

impl CompactSize {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

impl Encodable for CompactSize {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        if self.0 < 253 {
            let n = self.0 as u8;
            writer.write(&[n])
        } else if self.0 <= u16::MAX as u64 {
            let n = self.0 as u16;
            let written = writer.write(&[253])?;
            writer.write(&n.to_le_bytes()).map(|n| n + written)
        } else if self.0 <= u32::MAX as u64 {
            let n = self.0 as u32;
            let written = writer.write(&[254])?;
            writer.write(&n.to_le_bytes()).map(|n| n + written)
        } else {
            let written = writer.write(&[255])?;
            writer.write(&self.0.to_le_bytes()).map(|n| n + written)
        }
    }
}

impl Decodable for CompactSize {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let size = reader.read_u8()?;
        if size < 253 {
            Ok(Self(size as u64))
        } else if size == 253 {
            let num = reader.read_u16()?;
            Ok(Self(num as u64))
        } else if size == 254 {
            let num = reader.read_u32()?;
            Ok(Self(num as u64))
        } else {
            let num = reader.read_u64()?;
            Ok(Self(num))
        }
    }
}

impl From<CompactSize> for u64 {
    fn from(var_int: CompactSize) -> Self {
        var_int.0
    }
}

impl From<u64> for CompactSize {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<u32> for CompactSize {
    fn from(value: u32) -> Self {
        Self(value as u64)
    }
}

impl From<u16> for CompactSize {
    fn from(value: u16) -> Self {
        Self(value as u64)
    }
}

impl From<u8> for CompactSize {
    fn from(value: u8) -> Self {
        Self(value as u64)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn round_trips() {
        let cases = vec![
            0_u64,
            252,
            253,
            254,
            255,
            256,
            u16::MAX as u64 - 1,
            u16::MAX as u64,
            u16::MAX as u64 + 1,
            u32::MAX as u64 - 1,
            u32::MAX as u64,
            u32::MAX as u64 + 1,
            u64::MAX - 1,
            u64::MAX,
        ];

        for num in cases {
            let compact_size = CompactSize(num);
            let mut encoded = Vec::new();
            compact_size.consensus_encode(&mut encoded).expect("encode");
            let mut bytes = encoded.as_slice();
            let decoded = CompactSize::consensus_decode(&mut bytes).expect("decode");
            assert_eq!(compact_size, decoded, "decode {:?} -> {}", encoded, num);
        }
    }
}
