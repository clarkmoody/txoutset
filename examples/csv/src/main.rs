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

fn main() {
    let args = Args::parse();

    match Dump::new(&args.file, args.addresses) {
        Ok(dump) => {
            if args.check {
                return println!(
                    "Dump opened.\n Block Hash: {}\n UTXO Set Size: {}",
                    dump.block_hash, dump.coins_count
                );
            }
            for item in dump.into_iter() {
                let address = item.address.map_or(String::new(), |a| format!(",{}", a));
                println!(
                    "{},{},{},{},{}{}",
                    item.out_point,
                    u8::from(item.is_coinbase),
                    item.height,
                    item.amount.0,
                    hex::encode(item.script_buf.as_bytes()),
                    address
                );
            }
        }
        Err(e) => {
            eprintln!("{}: {}", e, args.file);
        }
    }
}
