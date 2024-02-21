use std::io::Write;

use clap::Parser;
use txoutset::Dump;

/// Parse the UTXO set dump file and output each entry as CSV
///
/// Each line of the output has the following columns:
///
/// - Out Point (txid:vout)
/// - Is Coinbase (0 - no, 1 - yes)
/// - Block Height
/// - Amount (satoshis)
/// - Script Public Key
/// - [optional] Address (specify -a)
#[derive(Debug, Parser)]
#[command(verbatim_doc_comment)]
struct Args {
    /// File containing the results of Bitcoin Core RPC `dumptxoutset`
    file: String,
    /// Compute addresses for each script pubkey
    #[arg(short, long, default_value_t = false)]
    addresses: bool,
    /// Check that the file exists and print simple metadata about the snapshot
    #[arg(short, long, default_value_t = false)]
    check: bool,
}

fn main() -> Result<(), std::io::Error> {
    let args = Args::parse();

    let mut stdout = std::io::stdout();

    match Dump::new(&args.file, args.addresses) {
        Ok(dump) => {
            if args.check {
                return writeln!(
                    stdout,
                    "Dump opened.\n Block Hash: {}\n UTXO Set Size: {}",
                    dump.block_hash, dump.utxo_set_size
                );
            }
            for item in dump {
                let address = match (args.addresses, item.address) {
                    (true, Some(address)) => {
                        format!(",{}", address)
                    }
                    (true, None) => ",".to_string(),
                    (false, _) => String::new(),
                };
                let r = writeln!(
                    stdout,
                    "{},{},{},{},{}{}",
                    item.out_point,
                    u8::from(item.is_coinbase),
                    item.height,
                    u64::from(item.amount),
                    hex::encode(item.script_pubkey.as_bytes()),
                    address
                );
                if let Err(e) = r {
                    if matches!(e.kind(), std::io::ErrorKind::BrokenPipe) {
                        break;
                    }
                }
            }

            Ok(())
        }
        Err(e) => {
            writeln!(std::io::stderr(), "{}: {}", e, args.file)
        }
    }
}
