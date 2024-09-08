use bitcoin::consensus::encode::Error;
use bitcoin::consensus::Decodable;
use bitcoin::hashes::Hash;
use bitcoin::script::{Builder, ScriptBuf};
use bitcoin::{opcodes, PubkeyHash, PublicKey, ScriptHash};

const NUM_SPECIAL_SCRIPTS: usize = 6;
const MAX_SCRIPT_SIZE: usize = 10_000;

use crate::VarInt;

/// Wrapper to enable script decompression
#[derive(Debug)]
pub struct Script(ScriptBuf);

impl Script {
    /// Reveal the inner script buffer
    pub fn into_inner(self) -> ScriptBuf {
        self.0
    }
}

impl From<ScriptBuf> for Script {
    fn from(script_buf: ScriptBuf) -> Self {
        Self(script_buf)
    }
}

impl Decodable for Script {
    fn consensus_decode<R: bitcoin::io::BufRead + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let mut size = u64::from(VarInt::consensus_decode(reader)?) as usize;

        match size {
            0x00 => {
                // P2PKH
                let mut bytes = [0; 20];
                reader.read_exact(&mut bytes)?;
                let pubkey_hash =
                    PubkeyHash::from_slice(&bytes).map_err(|_| Error::ParseFailed("HASH-160"))?;
                Ok(Script(ScriptBuf::new_p2pkh(&pubkey_hash)))
            }
            0x01 => {
                // P2SH
                let mut bytes = [0; 20];
                reader.read_exact(&mut bytes)?;
                let script_hash =
                    ScriptHash::from_slice(&bytes).map_err(|_| Error::ParseFailed("HASH-160"))?;
                Ok(Script(ScriptBuf::new_p2sh(&script_hash)))
            }
            0x02 | 0x03 => {
                // P2PK (compressed)
                let mut bytes = [0; 32];
                reader.read_exact(&mut bytes)?;

                let mut script_bytes = Vec::with_capacity(35);
                script_bytes.push(opcodes::all::OP_PUSHBYTES_33.to_u8());
                script_bytes.push(size as u8);
                script_bytes.extend_from_slice(&bytes);
                script_bytes.push(opcodes::all::OP_CHECKSIG.to_u8());

                Ok(Script(ScriptBuf::from(script_bytes)))
            }
            0x04 | 0x05 => {
                // P2PK (uncompressed)
                let mut bytes = [0; 32];
                reader.read_exact(&mut bytes)?;

                let mut compressed_pubkey_bytes = Vec::with_capacity(33);
                compressed_pubkey_bytes.push((size - 2) as u8);
                compressed_pubkey_bytes.extend_from_slice(&bytes);

                let compressed_pubkey = PublicKey::from_slice(&compressed_pubkey_bytes)
                    .map_err(|_| Error::ParseFailed("parse public key"))?;
                let inner_uncompressed = compressed_pubkey.inner.serialize_uncompressed();

                let mut script_bytes = Vec::with_capacity(67);
                script_bytes.push(opcodes::all::OP_PUSHBYTES_65.to_u8());
                script_bytes.extend_from_slice(&inner_uncompressed);
                script_bytes.push(opcodes::all::OP_CHECKSIG.to_u8());

                Ok(Script(ScriptBuf::from(script_bytes)))
            }
            _ => {
                size -= NUM_SPECIAL_SCRIPTS;
                let mut bytes = Vec::with_capacity(size);
                bytes.resize_with(size, || 0);
                if size > MAX_SCRIPT_SIZE {
                    reader.read_exact(&mut bytes)?;
                    let script = Builder::new()
                        .push_opcode(opcodes::all::OP_RETURN)
                        .into_script();
                    Ok(Script(script))
                } else {
                    reader.read_exact(&mut bytes)?;
                    Ok(Script(ScriptBuf::from_bytes(bytes)))
                }
            }
        }
    }
}
