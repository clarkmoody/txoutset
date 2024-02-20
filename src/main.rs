use clap::Parser;
use txoutset::Dump;

#[derive(Debug, Parser)]
struct Args {
    file: String,
    /// Compute addresses for each script pubkey
    #[arg(short, long, default_value_t = false)]
    addresses: bool,
}

fn main() {
    let args = Args::parse();

    match Dump::new(&args.file, args.addresses) {
        Ok(dump) => {
            println!(
                "Dump opened.\n Block Hash: {}\n UTXO Set Size: {}",
                dump.block_hash, dump.coins_count
            );
            let mut c = 0;
            for item in dump.into_iter() {
                if item.script_buf.is_empty() {
                    println!("[{c}]: {:#?}", item);
                    break;
                }
                c += 1;
            }
            println!("Processed {c} items.");
        }
        Err(e) => {
            eprintln!("{}: {}", e, args.file);
        }
    }
}
