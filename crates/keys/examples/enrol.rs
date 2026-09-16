//! What setting up a device actually looks like.
//!
//! ```bash
//! cargo run -p qurb-keys --example enrol -- /tmp/device-a
//! cargo run -p qurb-keys --example enrol -- /tmp/device-b "word word word ..."
//! ```
//!
//! With one argument it creates a key and shows the phrase. With a phrase as a
//! second argument it restores instead — which is what a replacement device
//! does.

use qurb_keys::{Opened, Purpose, RecoveryPhrase, Vault};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let Some(dir) = args.get(1).map(PathBuf::from) else {
        eprintln!("usage: enrol <dir> [recovery phrase]");
        std::process::exit(2);
    };

    let vault = Vault::at(&dir);

    let key = match args.get(2) {
        Some(words) => {
            let phrase = RecoveryPhrase::parse(words)?;
            let key = vault.restore(&phrase)?;
            println!("Restored from a recovery phrase.\n");
            key
        }
        None => match vault.open_or_create()? {
            Opened::Existing(key) => {
                println!("This device already has a key.\n");
                println!("The recovery phrase cannot be shown again — it exists only at the");
                println!("moment the key is created. If it was not written down then, the way");
                println!("to get a new one is to start over with a new key, which abandons");
                println!("everything encrypted under this one.\n");
                key
            }
            Opened::Created { key, phrase } => {
                println!("A new key was created for this device.\n");
                println!("{}", "=".repeat(68));
                println!("{}", phrase.numbered());
                println!("{}", "=".repeat(68));
                println!();
                println!("Write these 24 words down on paper, in order, now.");
                println!();
                println!("They are not a backup of your key. They ARE your key, in a form you");
                println!("can hold. Nobody else has a copy — not us, not a server. If you lose");
                println!("them and lose this device, your files cannot be recovered by anyone,");
                println!("including us. That is not a policy we could choose to relax.");
                println!();
                key
            }
        },
    };

    // Proof the key works, without printing any of it.
    let chunk = key.derive(Purpose::ChunkEncryption);
    println!("key file    {}", vault.path().display());
    println!("chunk key   derived, {} bytes, not shown", chunk.as_bytes().len());
    Ok(())
}
