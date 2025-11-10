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
use bitcoin::p2p::Magic;
use bitcoin::{Address, BlockHash, OutPoint, ScriptBuf, Txid};
use thiserror::Error;

pub mod amount;
pub mod compact_size;
pub mod script;
pub mod var_int;
pub use amount::Amount;
pub use compact_size::CompactSize;
pub use script::Script;
pub use var_int::VarInt;

const SNAPSHOT_MAGIC: [u8; 5] = [b'u', b't', b'x', b'o', 0xff];

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
    /// Optionally compute addresses using this Network
    address_network: Option<bitcoin::Network>,
    /// The block hash of the chain tip when the UTXO set was exported
    pub block_hash: BlockHash,
    /// The data source for the dump
    reader: R,
    /// Internal state tracking for non-legacy dump files
    state: State,
    /// Number of entries in the dump file
    pub utxo_set_size: u64,
}

/// Internal state for non-legacy dumps
enum State {
    /// Working through a list of out points for the same TXID
    HaveTxid {
        txid: Txid,
        out_points_remaining: u64,
    },
    /// Looking for the next TXID in the dump
    NeedTxid,
    /// No state tracking needed
    Legacy,
}

/// Whether to compute addresses while processing.
#[derive(Debug, Default)]
pub enum ComputeAddresses {
    /// Do not compute addresses.
    #[default]
    No,
    /// Compute addresses for a given network
    Yes(Network),
}

#[derive(Debug, Default)]
pub enum Network {
    /// Detect the network from the dump file (works after Core 28.0)
    #[default]
    Detect,
    /// Specify which network to use
    Specify(bitcoin::Network),
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Problem decoding a Bitcoin library structure
    #[error("Decode: {0}")]
    ConsensusDecode(#[from] bitcoin::consensus::encode::Error),
    /// Standard I/O Error
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Cannot detect network for legacy dump formats")]
    NetworkDetect,
    /// Network mismatch between specified and detected network
    #[error("Specified network ({specified}) does not match detected network ({detected})")]
    NetworkMismatch {
        detected: bitcoin::Network,
        specified: bitcoin::Network,
    },
    /// Got a version number we don't know how to process
    #[error("Unknown version number: {0}")]
    UnknownVersion(u16),
    /// Unknown magic bytes in the dump file
    #[error("Unknown magic bytes: {0}")]
    UnknownMagic(#[from] bitcoin::p2p::UnknownMagicError),
}

impl<R> Dump<R>
where
    R: Read + Seek,
{
    /// Decode the data from a reader
    pub fn from_reader(mut reader: R, compute_addresses: ComputeAddresses) -> Result<Self, Error> {
        // Look for magic bytes at the start of the stream
        let mut possible_magic = [0_u8; 5];
        reader.read_exact(&mut possible_magic)?;

        let mut state = State::NeedTxid;
        let address_network;

        // Snapshot from Core 28.0 or later starts with magic bytes
        if possible_magic == SNAPSHOT_MAGIC {
            let version = u16::consensus_decode(&mut reader)?;
            if version != 2 {
                return Err(Error::UnknownVersion(version));
            }
            // Network magic
            let magic = Magic::consensus_decode(&mut reader)?;
            let network = bitcoin::Network::try_from(magic)?;

            address_network = match compute_addresses {
                ComputeAddresses::No => None,
                ComputeAddresses::Yes(Network::Detect) => Some(network),
                ComputeAddresses::Yes(Network::Specify(specified)) if specified == network => {
                    Some(network)
                }
                ComputeAddresses::Yes(Network::Specify(specified)) => {
                    return Err(Error::NetworkMismatch {
                        detected: network,
                        specified,
                    });
                }
            };
        } else {
            reader.rewind()?;
            state = State::Legacy;
            address_network = match compute_addresses {
                ComputeAddresses::No => None,
                ComputeAddresses::Yes(Network::Specify(network)) => Some(network),
                ComputeAddresses::Yes(Network::Detect) => {
                    return Err(Error::NetworkDetect);
                }
            }
        }

        let block_hash = BlockHash::consensus_decode(&mut reader)?;
        let utxo_set_size = u64::consensus_decode(&mut reader)?;

        Ok(Self {
            address_network,
            block_hash,
            reader: reader,
            state,
            utxo_set_size,
        })
    }
}

impl Dump<File> {
    /// Opens a UTXO set dump from a file path
    pub fn new(path: impl AsRef<Path>, compute_addresses: ComputeAddresses) -> Result<Self, Error> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(Error::Io(std::io::Error::from(ErrorKind::NotFound)));
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
        let out_point = match self.state {
            State::HaveTxid {
                txid,
                out_points_remaining,
            } => {
                let vout = u64::from(CompactSize::consensus_decode(&mut self.reader).ok()?) as u32;
                let out_points_remaining = out_points_remaining.saturating_sub(1);
                if out_points_remaining == 0 {
                    self.state = State::NeedTxid;
                } else {
                    self.state = State::HaveTxid {
                        txid,
                        out_points_remaining,
                    };
                }

                OutPoint::new(txid, vout)
            }
            State::NeedTxid => {
                let txid = Txid::consensus_decode(&mut self.reader).ok()?;
                let out_points_remaining =
                    u64::from(CompactSize::consensus_decode(&mut self.reader).ok()?)
                        .saturating_sub(1);
                let vout = u64::from(CompactSize::consensus_decode(&mut self.reader).ok()?) as u32;
                if out_points_remaining > 0 {
                    self.state = State::HaveTxid {
                        txid,
                        out_points_remaining,
                    };
                }

                OutPoint::new(txid, vout)
            }
            State::Legacy => OutPoint::consensus_decode(&mut self.reader).ok()?,
        };

        let code = Code::consensus_decode(&mut self.reader).ok()?;

        let amount = Amount::consensus_decode(&mut self.reader).ok()?;

        let script_buf = Script::consensus_decode(&mut self.reader)
            .ok()?
            .into_inner();

        let address = self
            .address_network
            .and_then(|network| Address::from_script(script_buf.as_script(), network).ok());

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
        let dump = Dump::from_reader(
            &mut reader,
            ComputeAddresses::Yes(Network::Specify(bitcoin::Network::Signet)),
        )
        .expect("Load Dump 27.0");

        let last_tx_out = dump.into_iter().skip(99).next().expect("100th tx out");

        validate_tx_out(last_tx_out);
    }

    #[test]
    fn parse_dump_28() {
        let mut reader = Cursor::new(DUMP_28_0);
        let dump = Dump::from_reader(&mut reader, ComputeAddresses::Yes(Network::Detect))
            .expect("Load Dump 28.0");

        let last_tx_out = dump.into_iter().skip(99).next().expect("100th tx out");

        validate_tx_out(last_tx_out);
    }
}
