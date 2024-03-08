//! UTXO set dump parser
//!
//! ```skip
//! use txoutset::{ComputeAddresses, Dump};
//! let dump = Dump::new("utxo.bin", ComputeAddresses::No).unwrap();
//! for item in dump {
//!     println!("{}: {}", item.out_point, u64::from(item.amount));
//! }
//! ```

use std::io::{ErrorKind, Seek};
use std::path::Path;

use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::{Address, BlockHash, OutPoint, ScriptBuf};

pub use bitcoin::Network;

pub mod amount;
pub mod script;
pub mod var_int;
pub use amount::Amount;
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
/// The struct holds a file reference to the export and implements `Iterator`
/// to produce [`TxOut`] entries.
pub struct Dump {
    /// The block hash of the chain tip when the UTXO set was exported
    pub block_hash: BlockHash,
    compute_addresses: ComputeAddresses,
    file: std::fs::File,
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

impl Dump {
    /// Opens a UTXO set dump.
    pub fn new(
        path: impl AsRef<Path>,
        compute_addresses: ComputeAddresses,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(std::io::Error::from(ErrorKind::NotFound).into());
        }
        let mut file = std::fs::File::open(path)?;
        let block_hash = BlockHash::consensus_decode(&mut file)?;
        let utxo_set_size = u64::consensus_decode(&mut file)?;

        Ok(Self {
            block_hash,
            utxo_set_size,
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
                log::error!("[{}->{}] OutPoint decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let code = Code::consensus_decode(&mut self.file)
            .map_err(|e| {
                let pos = self.file.stream_position().unwrap_or_default();
                log::error!("[{}->{}] Code decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let amount = Amount::consensus_decode(&mut self.file)
            .map_err(|e| {
                let pos = self.file.stream_position().unwrap_or_default();
                log::error!("[{}->{}] Amount decode: {:?}", item_start_pos, pos, e);
                e
            })
            .ok()?;
        let script_buf = Script::consensus_decode(&mut self.file)
            .map_err(|e| {
                let pos = self.file.stream_position().unwrap_or_default();
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
        let code = u32::try_from(u64::from(var_int))
            .map_err(|_| bitcoin::consensus::encode::Error::ParseFailed("invalid cast to u32"))?;

        Ok(Code {
            height: code >> 1,
            is_coinbase: (code & 0x01) == 1,
        })
    }
}
