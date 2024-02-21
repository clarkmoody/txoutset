use std::io::{ErrorKind, Seek};
use std::path::PathBuf;

use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::{Address, BlockHash, OutPoint, ScriptBuf};

pub mod script;
pub mod var_int;
pub use var_int::VarInt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOut {
    pub address: Option<Address>,
    pub amount: Amount,
    pub height: u32,
    pub is_coinbase: bool,
    pub out_point: OutPoint,
    pub script_buf: ScriptBuf,
}

pub struct Dump {
    pub block_hash: BlockHash,
    pub coins_count: u64,
    compute_addresses: bool,
    file: std::fs::File,
}

impl Dump {
    pub fn new(
        path: impl Into<PathBuf>,
        compute_addresses: bool,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let path = path.into();
        if !path.exists() {
            return Err(std::io::Error::from(ErrorKind::NotFound).into());
        }
        let mut file = std::fs::File::open(path)?;
        let block_hash = BlockHash::consensus_decode(&mut file)?;
        let coins_count = u64::consensus_decode(&mut file)?;

        Ok(Self {
            block_hash,
            coins_count,
            compute_addresses,
            file,
        })
    }
}

impl Iterator for Dump {
    type Item = TxOut;

    fn next(&mut self) -> Option<Self::Item> {
        let item_start_pos = self.file.stream_position().unwrap_or_default();

        let out_point = OutPoint::consensus_decode(&mut self.file)
            .map_err(|e| {
                let pos = self.file.stream_position().unwrap_or_default();
                eprintln!("[{}->{}] OutPoint decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let code = Code::consensus_decode(&mut self.file)
            .map_err(|e| {
                let pos = self.file.stream_position().unwrap_or_default();
                eprintln!("[{}->{}] Code decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let amount = Amount::consensus_decode(&mut self.file)
            .map_err(|e| {
                let pos = self.file.stream_position().unwrap_or_default();
                eprintln!("[{}->{}] Amount decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let script_buf = script::Compressed::consensus_decode(&mut self.file)
            .map_err(|e| {
                let pos = self.file.stream_position().unwrap_or_default();
                eprintln!("[{}->{}] Script decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?
            .into_inner();

        let address = self
            .compute_addresses
            .then(|| Address::from_script(script_buf.as_script(), bitcoin::Network::Bitcoin).ok())
            .flatten();

        Some(TxOut {
            address,
            amount,
            height: code.height,
            is_coinbase: code.is_coinbase,
            out_point,
            script_buf,
        })
    }
}

#[derive(Debug)]
struct Code {
    height: u32,
    is_coinbase: bool,
}

impl Encodable for Code {
    fn consensus_encode<W: std::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, std::io::Error> {
        let code = self.height * 2 + u32::from(self.is_coinbase);
        let var_int = VarInt::from(code);

        var_int.consensus_encode(writer)
    }
}

impl Decodable for Code {
    fn consensus_decode<R: std::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let var_int = VarInt::consensus_decode(reader)?;
        let code = u32::try_from(var_int.0)
            .map_err(|_| bitcoin::consensus::encode::Error::ParseFailed("invalid cast to u32"))?;

        Ok(Code {
            height: code >> 1,
            is_coinbase: (code & 0x01) == 1,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(pub u64);

impl Amount {
    pub fn compress(&self) -> CompressedAmount {
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
            assert!(d >= 1 && d <= 9);
            n /= 10;
            1 + (n * 9 + d - 1) * 10 + e
        } else {
            1 + (n - 1) * 10 + 9
        };

        CompressedAmount(x)
    }
}

impl Encodable for Amount {
    fn consensus_encode<W: std::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, std::io::Error> {
        let compressed = self.compress();
        let var_int = VarInt::from(compressed);

        var_int.consensus_encode(writer)
    }
}

impl Decodable for Amount {
    fn consensus_decode<R: std::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let var_int = VarInt::consensus_decode(reader)?;
        let compressed = CompressedAmount::from(var_int);

        Ok(compressed.decompress())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompressedAmount(u64);

impl CompressedAmount {
    pub fn decompress(&self) -> Amount {
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
        CompressedAmount(var_int.0)
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
    fn amount_compression() {
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

    #[test]
    fn decompress_amount() {
        let compressed = CompressedAmount(0x77);

        assert_eq!(compressed.decompress(), Amount(13_0000_0000));
    }

    #[test]
    fn varint_encoding() {
        let pairs = vec![
            (0, vec![0x00]),
            (1, vec![0x01]),
            (127, vec![0x7f]),
            (128, vec![0x80, 0x00]),
            (255, vec![0x80, 0x7f]),
            (256, vec![0x81, 0x00]),
            (16383, vec![0xfe, 0x7f]),
            (16384, vec![0xff, 0x00]),
            (16511, vec![0xff, 0x7f]),
            (65535, vec![0x82, 0xfe, 0x7f]),
        ];

        for (num, encoding) in pairs {
            let mut v = Vec::new();
            let var_int = VarInt(num);
            var_int.consensus_encode(&mut v).expect("encode");
            assert_eq!(v, encoding);
        }
    }
}
