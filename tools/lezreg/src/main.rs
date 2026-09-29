//! lezreg — host tool for the Provenance registry.
//!
//! Derives the same PDAs the guest derives, serializes instructions with the
//! same risc0-serde codec the guest deserializes, and (with `--send`) submits
//! through the LEZ wallet. Everything it is about to do is printed first, so a
//! submission is always a reviewed artifact.

use std::str::FromStr;

mod instruction;

use nssa::program::Program;
use nssa_core::account::AccountId;
use nssa_core::program::ProgramId;
use spel_framework_core::pda::{compute_pda, seed_from_str, ToSeed};
use wallet::{AccountIdentity, WalletCore};

use instruction::Instruction;

/// ImageID of the deployed registry (testnet). This is the **callee** program, and the
/// namespace every registry account lives in: the state machine derives PDA claims
/// against `chained_call.program_id` and assigns that same id as `program_owner`
/// (`lee/state_machine/src/validated_state_diff/mod.rs:236`, LEZ `v0.2.4` / `47eba25`).
///
/// `--image-id` is the *subject* being registered, which is only seed data. The two
/// coincide for the registry's self-entry — which is why deriving one id from the other
/// looked fine until a non-self registration was actually tried.
const DEFAULT_REGISTRY_IMAGE_ID: &str =
    "7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6";

fn usage() {
    eprintln!("usage: lezreg <register|attest|attach-manifest|index-init|index-append> [flags] [--send]");
    eprintln!("  --image-id <hex32>          subject program (seed data, not the callee)");
    eprintln!("  --registry-image-id <hex32> deployed registry; defaults to the testnet one");
    eprintln!("  --authority <base58>        signing account");
    eprintln!("  register needs    --name --version --author-name --description  (optional --tags, --idl-cid)");
    eprintln!("  attest needs      --label");
    eprintln!("  attach-manifest needs --source-cid --manifest-cid --builder-image-digest --dep-audit-hash");
    eprintln!("                          (optional --repo-url --commit; pass 'unset' to store zeros)");
    eprintln!("  index-* need      --bucket --shard  (index-append also --image-id)");
}

/// A CLI read by auditors and CI reports a bad invocation as a message and exit 2 — never
/// as a thread panic, which is unreadable in a pipeline and hides which argument is wrong.
fn fail(msg: String) -> ! {
    eprintln!("error: {msg}");
    usage();
    std::process::exit(2)
}

fn bytes32(raw: &str, ctx: &str) -> [u8; 32] {
    let s = raw.strip_prefix("0x").unwrap_or(raw);
    let decoded = match hex::decode(s) {
        Ok(d) => d,
        Err(e) => fail(format!("{ctx}: {e}")),
    };
    let len = decoded.len();
    match decoded.try_into() {
        Ok(b) => b,
        Err(_) => fail(format!("{ctx}: expected 32 bytes (64 hex chars), got {len} bytes")),
    }
}

/// ImageID bytes -> the chain's `ProgramId` (8 little-endian u32 limbs).
fn program_id(image: &[u8; 32]) -> ProgramId {
    let mut limbs = [0u32; 8];
    for (i, chunk) in image.chunks(4).enumerate() {
        limbs[i] = u32::from_le_bytes(chunk.try_into().unwrap());
    }
    limbs
}

fn account(raw: &str) -> AccountId {
    AccountId::from_str(raw)
        .unwrap_or_else(|e| fail(format!("--authority: bad account id '{raw}': {e}")))
}

fn arg(args: &[String], flag: &str) -> Option<String> {
    let next = args
        .iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned();
    // `--authority --name X` is a typo for a missing value, not a value of "--name".
    if let Some(v) = &next {
        if v.starts_with("--") {
            fail(format!("{flag} needs a value, got the next flag '{v}'"));
        }
    }
    next
}

fn wanted(args: &[String], flag: &str) -> String {
    arg(args, flag).unwrap_or_else(|| fail(format!("missing required flag {flag}")))
}

/// Required u32 flag, reported the way an operator can act on it.
fn wanted_u32(args: &[String], flag: &str) -> u32 {
    let raw = wanted(args, flag);
    raw.parse()
        .unwrap_or_else(|e| fail(format!("{flag}: '{raw}' is not a u32 ({e})")))
}

/// A Tier-3 manifest digest. These fields are *stated*, never defaulted: the account stores bare
/// `[u8; 32]`s, so an all-zero value is indistinguishable on chain from an entry that never had a
/// manifest attached — an accident must not be able to produce it. `unset` is the explicit token,
/// and using it says so out loud.
fn digest_arg(args: &[String], flag: &str) -> [u8; 32] {
    match arg(args, flag) {
        Some(v) if v == "unset" => {
            eprintln!(
                "warning: {flag} stored as zeros — on chain this is indistinguishable from an \
                 entry that never had a manifest"
            );
            [0u8; 32]
        }
        Some(v) => bytes32(&v, flag),
        None => fail(format!("{flag} is required (pass the literal 'unset' to store zeros deliberately)")),
    }
}

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// `Vec<Vec<u8>>` tags from a comma-separated list.
fn tags(raw: &str) -> Vec<Vec<u8>> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.as_bytes().to_vec())
        .collect()
}

fn entry_pda(pid: &ProgramId, image: &[u8; 32]) -> AccountId {
    compute_pda(pid, &[&seed_from_str("entry"), image])
}
fn claim_pda(pid: &ProgramId, image: &[u8; 32], authority: &AccountId) -> AccountId {
    compute_pda(
        pid,
        &[
            &seed_from_str("claim3p"),
            image,
            authority.value(),
        ],
    )
}

fn index_pda(pid: &ProgramId, bucket: u32, shard: u32) -> AccountId {
    compute_pda(
        pid,
        &[&seed_from_str("index"), &bucket.to_seed(), &shard.to_seed()],
    )
}

#[tokio::main]
async fn main() {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let cmd = all.first().cloned().unwrap_or_else(|| {
        usage();
        std::process::exit(2);
    });
    let rest = &all[1..];
    let image = arg(rest, "--image-id").map(|s| bytes32(&s, "--image-id"));
    let authority = account(&wanted(rest, "--authority"));
    let registry = arg(rest, "--registry-image-id")
        .map(|s| bytes32(&s, "--registry-image-id"))
        .unwrap_or_else(|| bytes32(DEFAULT_REGISTRY_IMAGE_ID, "--registry-image-id"));
    // The callee, and therefore the PDA namespace — never the subject's ImageID.
    let pid = program_id(&registry);

    println!("registry (callee)     {}", hex::encode(&registry));
    println!("subject  (ImageID)    {}", match &image {
        Some(i) => hex::encode(i),
        None => "<none: this instruction takes no subject>".to_string(),
    });
    println!("program_id limbs      {pid:?}");
    if image.as_ref() == Some(&registry) {
        println!("note                  subject == registry (self-entry): both derivations agree here");
    }

    let (instruction, accounts): (Instruction, Vec<AccountId>) = match cmd.as_str() {
        "register" => {
            let image = match image {
                Some(i) => i,
                None => fail("register needs --image-id".into()),
            };
            let entry = entry_pda(&pid, &image);
            (
                Instruction::Register {
                    image_id: image,
                    name: wanted(rest, "--name").into_bytes(),
                    version: wanted(rest, "--version").into_bytes(),
                    author_name: wanted(rest, "--author-name").into_bytes(),
                    description: wanted(rest, "--description").into_bytes(),
                    tags: tags(&arg(rest, "--tags").unwrap_or_default()),
                    idl_cid: arg(rest, "--idl-cid")
                        .map(|s| bytes32(&s, "--idl-cid"))
                        .unwrap_or([0u8; 32]),
                },
                vec![entry, authority, clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID],
            )
        }
        "attest" => {
            let image = match image {
                Some(i) => i,
                None => fail("attest needs --image-id".into()),
            };
            let claim = claim_pda(&pid, &image, &authority);
            (
                Instruction::Attest {
                    image_id: image,
                    label: wanted(rest, "--label").into_bytes(),
                },
                vec![claim, authority, clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID],
            )
        }
        "attach-manifest" => {
            let image = match image {
                Some(i) => i,
                None => fail("attach-manifest needs --image-id".into()),
            };
            let entry = entry_pda(&pid, &image);
            (
                Instruction::AttachManifest {
                    image_id: image,
                    repo_url: arg(rest, "--repo-url").unwrap_or_default().into_bytes(),
                    commit: digest_arg(rest, "--commit"),
                    source_cid: digest_arg(rest, "--source-cid"),
                    manifest_cid: digest_arg(rest, "--manifest-cid"),
                    builder_image_digest: digest_arg(rest, "--builder-image-digest"),
                    dep_audit_hash: digest_arg(rest, "--dep-audit-hash"),
                },
                // same account layout as register: the entry PDA, its authority as signer, the clock
                vec![entry, authority, clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID],
            )
        }
        "index-init" | "index-append" => {
            let bucket = wanted_u32(rest, "--bucket");
            let shard = wanted_u32(rest, "--shard");
            let index = index_pda(&pid, bucket, shard);
            let ix = if cmd == "index-init" {
                Instruction::IndexInit { bucket, shard }
            } else {
                let image_id = match image {
                    Some(i) => i,
                    None => fail("index-append needs --image-id".into()),
                };
                Instruction::IndexAppend {
                    image_id,
                    bucket,
                    shard,
                }
            };
            // index PDAs are not authority-scoped: no signer beyond the clock.
            (ix, vec![index, authority, clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID])
        }
        other => fail(format!("unknown command '{other}'")),
    };

    let data: Vec<u32> = Program::serialize_instruction(&instruction)
        .expect("instruction must serialize")
        .to_vec();

    println!("accounts              {}", accounts.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", "));
    println!("instruction words     {} u32 words", data.len());
    println!("instruction_data      {}", serde_json::to_string(&data).unwrap());

    if !has(rest, "--send") {
        println!("\ndry run: nothing submitted (add --send to submit)");
        return;
    }

    let wallet_core = match WalletCore::from_env().await {
        Ok(w) => w,
        Err(e) => fail(format!(
            "wallet init: {e} (LEE_WALLET_HOME_DIR must point at the real wallet directory)"
        )),
    };
    let identities = vec![
        AccountIdentity::PublicNoSign(accounts[0]),
        AccountIdentity::Public(authority),
        AccountIdentity::PublicNoSign(accounts[2]),
    ];
    println!("\nsubmitting {cmd} ...");
    match wallet_core.send_pub_tx(identities, data, pid).await {
        Ok(hash) => println!("tx hash: {hash:?}"),
        Err(e) => {
            // A non-zero exit is the only thing a script can read.
            eprintln!("error: submission failed: {e}");
            std::process::exit(1);
        }
    }
}
