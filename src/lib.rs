//! UTXO set dump parser
//!
//! ```skip
//! use txoutset::{ComputeAddresses, Dump};
//! let dump = Dump::new("utxo.bin", ComputeAddresses::No).unwrap();
//! for item in dump {
//!     println!("{}: {}", item.out_point, u64::from(item.amount));
//! }
//! ```

use std::fs::File;
use std::io::{ErrorKind, Read, Seek, Write};
use std::path::Path;

use bitcoin::consensus::{Decodable, Encodable};
pub use bitcoin::Network;
use bitcoin::{Address, BlockHash, OutPoint, ScriptBuf};

pub mod amount;
pub mod compact_size;
pub mod script;
pub mod var_int;
pub use amount::Amount;
pub use compact_size::CompactSize;
pub use script::Script;
pub use var_int::VarInt;

/// An unspent transaction output entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOut {
    /// The address form of the script public key
    pub address: Option<Address>,
    /// Value of the output, satoshis
    pub amount: Amount,
    /// Block height where the transaction was confirmed
    pub height: u32,
    /// Whether the output is in the coinbase transaction of the block
    pub is_coinbase: bool,
    /// The specific transaction output
    pub out_point: OutPoint,
    /// The script public key
    pub script_pubkey: ScriptBuf,
}

/// The UTXO set dump parser helper struct
///
/// The struct holds a reader containing the export and implements `Iterator`
/// to produce [`TxOut`] entries.
pub struct Dump<R>
where
    R: Read + Seek,
{
    /// The block hash of the chain tip when the UTXO set was exported
    pub block_hash: BlockHash,
    compute_addresses: ComputeAddresses,
    reader: R,
    /// Number of entries in the dump file
    pub utxo_set_size: u64,
}

/// Whether to compute addresses while processing.
#[derive(Debug, Default)]
pub enum ComputeAddresses {
    /// Do not compute addresses.
    #[default]
    No,
    /// Compute addresses and assume a particular network.
    Yes(bitcoin::Network),
}

impl<R> Dump<R>
where
    R: Read + Seek,
{
    /// Decode the data from a reader
    pub fn from_reader(
        mut reader: R,
        compute_addresses: ComputeAddresses,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let block_hash = BlockHash::consensus_decode(&mut reader)?;
        let utxo_set_size = u64::consensus_decode(&mut reader)?;

        Ok(Self {
            block_hash,
            utxo_set_size,
            compute_addresses,
            reader: reader,
        })
    }
}

impl Dump<File> {
    /// Opens a UTXO set dump from a file path
    pub fn new(
        path: impl AsRef<Path>,
        compute_addresses: ComputeAddresses,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(std::io::Error::from(ErrorKind::NotFound).into());
        }
        let file = File::open(path)?;

        Dump::from_reader(file, compute_addresses)
    }
}

impl<R> Iterator for Dump<R>
where
    R: Read + Seek,
{
    type Item = TxOut;

    fn next(&mut self) -> Option<Self::Item> {
        let item_start_pos = self.reader.stream_position().unwrap_or_default();

        let out_point = OutPoint::consensus_decode(&mut self.reader)
            .map_err(|e| {
                let pos = self.reader.stream_position().unwrap_or_default();
                log::error!("[{}->{}] OutPoint decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let code = Code::consensus_decode(&mut self.reader)
            .map_err(|e| {
                let pos = self.reader.stream_position().unwrap_or_default();
                log::error!("[{}->{}] Code decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let amount = Amount::consensus_decode(&mut self.reader)
            .map_err(|e| {
                let pos = self.reader.stream_position().unwrap_or_default();
                log::error!("[{}->{}] Amount decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let script_buf = Script::consensus_decode(&mut self.reader)
            .map_err(|e| {
                let pos = self.reader.stream_position().unwrap_or_default();
                log::error!("[{}->{}] Script decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?
            .into_inner();

        let address = match &self.compute_addresses {
            ComputeAddresses::No => None,
            ComputeAddresses::Yes(network) => {
                Address::from_script(script_buf.as_script(), *network).ok()
            }
        };

        Some(TxOut {
            address,
            amount,
            height: code.height,
            is_coinbase: code.is_coinbase,
            out_point,
            script_pubkey: script_buf,
        })
    }
}

#[derive(Debug)]
struct Code {
    height: u32,
    is_coinbase: bool,
}

impl Encodable for Code {
    fn consensus_encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let code = self.height * 2 + u32::from(self.is_coinbase);
        let var_int = VarInt::from(code);

        var_int.consensus_encode(writer)
    }
}

impl Decodable for Code {
    fn consensus_decode<R: Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let var_int = VarInt::consensus_decode(reader)?;
        let code = u32::try_from(u64::from(var_int))
            .map_err(|_| bitcoin::consensus::encode::Error::ParseFailed("invalid cast to u32"))?;

        Ok(Code {
            height: code >> 1,
            is_coinbase: (code & 0x01) == 1,
        })
    }
}

#[cfg(test)]
mod test {
    use super::{ComputeAddresses, Dump, Network, TxOut};
    use std::io::Cursor;

    const DUMP_27_0: &[u8] = include_bytes!("../test/dump-27_0.dat");
    const DUMP_28_0: &[u8] = include_bytes!("../test/dump-28_0.dat");

    // The 100th tx out in the dump files
    fn validate_tx_out(tx_out: TxOut) {
        let address = tx_out.address.map(|a| a.to_string()).expect("address");
        assert_eq!(&address, "tb1qsajw7zxldhf6lg8rg3ru0d26n633gldzutjcwr");
        assert_eq!(tx_out.height, 45);
        assert!(tx_out.is_coinbase);
    }

    #[test]
    fn parse_dump_27() {
        let mut reader = Cursor::new(DUMP_27_0);
        let dump = Dump::from_reader(&mut reader, ComputeAddresses::Yes(Network::Signet))
            .expect("Load Dump 27.0");

        let last_tx_out = dump.into_iter().skip(99).next().expect("100th tx out");

        validate_tx_out(last_tx_out);
    }

    #[test]
    fn parse_dump_28() {
        let mut reader = Cursor::new(DUMP_28_0);
        let dump = Dump::from_reader(&mut reader, ComputeAddresses::Yes(Network::Signet))
            .expect("Load Dump 28.0");

        let last_tx_out = dump.into_iter().skip(99).next().expect("100th tx out");

        validate_tx_out(last_tx_out);
    }
}
