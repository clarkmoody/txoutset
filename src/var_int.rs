use bitcoin::consensus::{Decodable, Encodable, ReadExt};

/// Variable-length Integers
///
/// Bytes are a MSB base-128 encoding of the number.
/// The high bit in each byte signifies whether another digit follows. To make
/// sure the encoding is one-to-one, one is subtracted from all but the last
/// digit. Thus, the byte sequence a[] with length len, where all but the last
/// byte has bit 128 set, encodes the number:
///  (a[len-1] & 0x7F) + sum(i=1..len-1, 128^i*((a[len-i-1] & 0x7F)+1))
///
/// Properties:
/// * Very small (0-127: 1 byte, 128-16511: 2 bytes, 16512-2113663: 3 bytes)
/// * Every integer has exactly one encoding
/// * Encoding does not depend on size of original integer type
/// * No redundancy: every (infinite) byte sequence corresponds to a list of
///   encoded integers.
///
/// ```text
/// 0:         [0x00]  256:        [0x81 0x00]
/// 1:         [0x01]  16383:      [0xFE 0x7F]
/// 127:       [0x7F]  16384:      [0xFF 0x00]
/// 128:  [0x80 0x00]  16511:      [0xFF 0x7F]
/// 255:  [0x80 0x7F]  65535: [0x82 0xFE 0x7F]
/// 2^32:           [0x8E 0xFE 0xFE 0xFF 0x00]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VarInt(u64);

impl VarInt {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

impl Encodable for VarInt {
    fn consensus_encode<W: std::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, std::io::Error> {
        let mut num = self.0;
        let mut bytes = Vec::with_capacity((std::mem::size_of::<u64>() * 8 + 6) / 7);

        let mut first = true;
        loop {
            let tmp = (num & 0x7f) | if first { 0x00 } else { 0x80 };
            bytes.push(tmp as u8);
            if num <= 0x7f {
                break;
            }
            num = (num >> 7) - 1;
            first = false;
        }

        let bytes: Vec<u8> = bytes.into_iter().rev().collect();
        writer.write(bytes.as_slice())
    }
}

impl Decodable for VarInt {
    fn consensus_decode<R: std::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let mut n: u64 = 0;

        loop {
            let b = reader.read_u8()? as u64;
            if n > u64::MAX >> 7 {
                return Err(bitcoin::consensus::encode::Error::NonMinimalVarInt);
            }
            n = (n << 7) | (b & 0x7f);
            if (b & 0x80) != 0 {
                if n == u64::MAX {
                    return Err(bitcoin::consensus::encode::Error::NonMinimalVarInt);
                }
                n += 1;
            } else {
                return Ok(Self(n));
            }
        }
    }
}

impl From<VarInt> for u64 {
    fn from(var_int: VarInt) -> Self {
        var_int.0
    }
}

impl From<u64> for VarInt {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<u32> for VarInt {
    fn from(value: u32) -> Self {
        Self(value as u64)
    }
}

impl From<u16> for VarInt {
    fn from(value: u16) -> Self {
        Self(value as u64)
    }
}

impl From<u8> for VarInt {
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
            (0, vec![0x00]),
            (256, vec![0x81, 0x00]),
            (1, vec![0x01]),
            (16383, vec![0xfe, 0x7f]),
            (127, vec![0x7f]),
            (16384, vec![0xff, 0x00]),
            (128, vec![0x80, 0x00]),
            (16511, vec![0xff, 0x7f]),
            (255, vec![0x80, 0x7f]),
            (65535, vec![0x82, 0xfe, 0x7f]),
            (1 << 32, vec![0x8e, 0xfe, 0xfe, 0xff, 0x00]),
        ];

        for (num, bytes) in cases {
            let var_int = VarInt(num);
            let mut encoded = Vec::new();
            var_int.consensus_encode(&mut encoded).expect("encode");
            assert_eq!(encoded, bytes, "encode {} -> {:?}", num, bytes);

            let mut encoded = bytes.as_slice();
            let var_int = VarInt::consensus_decode(&mut encoded).expect("decode");
            assert_eq!(var_int, VarInt(num), "decode {:?} -> {}", bytes, num);
        }
    }
}
