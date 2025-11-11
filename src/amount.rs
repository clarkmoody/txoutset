use bitcoin::consensus::{Decodable, Encodable};

use crate::VarInt;

/// A compressible amount of satoshis
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(u64);

impl Amount {
    pub fn new(n: u64) -> Self {
        Self(n)
    }

    fn compress(&self) -> CompressedAmount {
        let mut n = self.0;

        if n == 0 {
            return CompressedAmount(0);
        }

        let mut e = 0;
        while ((n % 10) == 0) && e < 9 {
            n /= 10;
            e += 1;
        }

        let x = if e < 9 {
            let d = n % 10;
            assert!((1..=9).contains(&d));
            n /= 10;
            1 + (n * 9 + d - 1) * 10 + e
        } else {
            1 + (n - 1) * 10 + 9
        };

        CompressedAmount(x)
    }
}

impl Encodable for Amount {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let compressed = self.compress();
        let var_int = VarInt::from(compressed);

        var_int.consensus_encode(writer)
    }
}

impl Decodable for Amount {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let var_int = VarInt::consensus_decode(reader)?;
        let compressed = CompressedAmount::from(var_int);

        Ok(compressed.decompress())
    }
}

impl From<u64> for Amount {
    fn from(sats: u64) -> Self {
        Self(sats)
    }
}

impl From<Amount> for u64 {
    fn from(amount: Amount) -> Self {
        amount.0
    }
}

impl From<bitcoin::Amount> for Amount {
    fn from(amount: bitcoin::Amount) -> Self {
        Self(amount.to_sat())
    }
}

impl From<Amount> for bitcoin::Amount {
    fn from(amount: Amount) -> Self {
        bitcoin::Amount::from_sat(amount.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CompressedAmount(u64);

impl CompressedAmount {
    fn decompress(&self) -> Amount {
        let mut x = self.0;

        if x == 0 {
            return Amount(0);
        }

        x -= 1;

        // x = 10*(9*n + d - 1) + e
        let mut e = x % 10;
        x /= 10;

        let mut n = if e < 9 {
            // x = 9*n + d - 1
            let d = (x % 9) + 1;
            x /= 9;
            // x = n
            x * 10 + d
        } else {
            x + 1
        };

        while e > 0 {
            n *= 10;
            e -= 1;
        }

        Amount(n)
    }
}

impl From<VarInt> for CompressedAmount {
    fn from(var_int: VarInt) -> Self {
        CompressedAmount(u64::from(var_int))
    }
}

impl From<CompressedAmount> for VarInt {
    fn from(compressed: CompressedAmount) -> Self {
        VarInt::from(compressed.0)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn round_trips() {
        let amounts = vec![
            Amount(0),
            Amount(1),
            Amount(1_2345_6789),
            Amount(6_2500_0000),
            Amount(12_5000_0000),
            Amount(50_0000_0000),
            Amount(20_999_999_9769_0000),
        ];

        for amount in amounts {
            assert_eq!(amount, amount.compress().decompress());
        }
    }
}
