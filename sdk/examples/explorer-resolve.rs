//! The read path a stranger actually needs: the LEZ explorer shows an ImageID, this prints
//! what the registry claims about that program — and the account it read it from, so the
//! answer can be re-derived by anyone.
//!
//! ```text
//! cargo run --example explorer-resolve -- 7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6
//! cargo run --example explorer-resolve -- <image-id-hex> --registry <registry-image-id-hex> --rpc https://testnet.lez.logos.co/
//! ```
//!
//! Exit codes are part of the contract: `0` registered, `3` no entry for that ImageID, `2`
//! bad invocation, `1` chain or decode failure. "Nobody has claimed this program" is an
//! answer callers branch on, so it is not reported as an error.
//!
//! `idl_cid` on the deployed entry is the IDL file's sha256 (measured 2026-09-28: equals
//! `sha256sum provenance-idl.json`), so it is printed as hex rather than dressed up as a CID
//! with a codec the chain never recorded.

use provenance_sdk::chain::{entry_pda, image_id_from_hex, Sequencer};
use provenance_sdk::{Entry, SdkError, UNSET_CID};

const USAGE: &str = "usage: explorer-resolve <image-id-hex-64> [--registry <image-id-hex>] [--rpc <url>]";

enum Failure {
    Usage,
    Chain(String),
}

impl From<SdkError> for Failure {
    fn from(e: SdkError) -> Self {
        Failure::Chain(e.to_string())
    }
}

/// A flag's value, refusing to read the *next* flag as that value — `--rpc --registry X`
/// would otherwise silently send the literal `--registry` to the gateway.
fn flag(args: &[String], name: &str) -> Result<Option<String>, Failure> {
    let Some(i) = args.iter().position(|a| a == name) else {
        return Ok(None);
    };
    match args.get(i + 1) {
        Some(v) if !v.starts_with("--") => Ok(Some(v.clone())),
        _ => Err(Failure::Usage),
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn digest_label(bytes: &[u8; 32]) -> String {
    if bytes == &UNSET_CID {
        "(unset)".to_string()
    } else {
        hex::encode(bytes)
    }
}

fn print_entry(entry: &Entry) {
    println!("  name            {}", text(&entry.name));
    println!("  version         {}", text(&entry.version));
    println!("  author_name     {}", text(&entry.author_name));
    println!("  description     {}", text(&entry.description));
    let tags: Vec<String> = entry.tags.iter().map(|t| text(t)).collect();
    println!("  tags            [{}]", tags.join(", "));
    println!("  authority       {}", hex::encode(entry.authority));
    println!("  idl_cid         {}", digest_label(&entry.idl_cid));
    println!("  source_cid      {}", digest_label(&entry.source_cid));
    println!("  manifest_cid    {}", digest_label(&entry.manifest_cid));
    println!("  repo_url        {}", text(&entry.repo_url));
    println!("  commit          {}", digest_label(&entry.commit));
    println!("  builder_digest  {}", digest_label(&entry.builder_image_digest));
    println!("  dep_audit_hash  {}", digest_label(&entry.dep_audit_hash));
    println!("  registered_at   {}", entry.registered_at);
    println!("  updated_at      {}", entry.updated_at);
    println!("  revision        {}", entry.revision);
    println!("  schema_version  {}", entry.schema_version);
}

fn run(args: &[String]) -> Result<bool, Failure> {
    let Some(first) = args.first() else {
        return Err(Failure::Usage);
    };
    if first.starts_with("--") {
        return Err(Failure::Usage);
    }
    let subject = image_id_from_hex(first).map_err(|e| Failure::Chain(format!("image id: {e}")))?;

    let rest = &args[1..];
    let registry = match flag(rest, "--registry")? {
        Some(v) => Some(
            image_id_from_hex(&v)
                .map_err(|e| Failure::Chain(format!("registry image id: {e}")))?,
        ),
        None => None,
    };
    let rpc = flag(rest, "--rpc")?;

    let mut sequencer = Sequencer::new(rpc);
    if let Some(image) = registry {
        sequencer = sequencer.with_registry(image);
    }

    println!("rpc             {}", sequencer.url());
    println!("tip block       {}", sequencer.last_block_id()?);
    println!("registry        {}", hex::encode(sequencer.registry()));
    println!(
        "subject         {}",
        hex::encode(subject)
    );
    println!(
        "entry account   {}   (derived in the registry's namespace; subject is seed data)",
        entry_pda(sequencer.registry(), &subject)
    );

    match sequencer.entry(&subject)? {
        Some(entry) => {
            println!("status          REGISTERED");
            print_entry(&entry);
            Ok(true)
        }
        None => {
            println!("status          UNREGISTERED (no entry account for this ImageID)");
            Ok(false)
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(found) => std::process::exit(if found { 0 } else { 3 }),
        Err(Failure::Usage) => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
        Err(Failure::Chain(msg)) => {
            eprintln!("error: {msg}");
            std::process::exit(1);
        }
    }
}
