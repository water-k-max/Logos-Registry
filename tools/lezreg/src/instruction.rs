//! The transaction ABI — a mirror of the enum `#[lez_program]` generates inside
//! the guest, declared host-side so `lezreg` (and later any write-side SDK) can
//! build transactions.
//!
//! Variant order is load-bearing: risc0-serde encodes an enum as its variant
//! index and the guest dispatcher has no catch-all arm. Field names must match
//! the guest's `#[instruction]` parameters (the generated match is a
//! named-struct pattern) and field order is the wire order. Accounts are
//! positional and never appear here.
//!
//! This enum deliberately does NOT live in `provenance_core`. Measured on the
//! golden pins: a `#[cfg(feature = "host")]`-gated copy inside the
//! guest-linked crate still changed the guest ELF (`a1a0fdc8…`/506,164 B →
//! `5f5b3d16…`/505,568 B, ImageID `7b4040d3…` → `fc70eb5b…`) even though the
//! guest never enables that feature, so the deployed ImageID would have been
//! hostage to host tooling edits. See IMPLEMENTATION_PLAN.md §0.9.
//!
//! `#[lez_program]` in its bare form generates the real enum inside the guest
//! module (host-invisible), so parity here is enforced by test, not by the
//! compiler: `idl_matches_host_mirror` compares this enum's variants, order and
//! field names against the IDL generated from the guest source.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instruction {
    Register {
        image_id: [u8; 32],
        name: Vec<u8>,
        version: Vec<u8>,
        author_name: Vec<u8>,
        description: Vec<u8>,
        tags: Vec<Vec<u8>>,
        idl_cid: [u8; 32],
    },
    Update {
        image_id: [u8; 32],
        name: Vec<u8>,
        version: Vec<u8>,
        author_name: Vec<u8>,
        description: Vec<u8>,
        tags: Vec<Vec<u8>>,
        idl_cid: [u8; 32],
    },
    AttachManifest {
        image_id: [u8; 32],
        repo_url: Vec<u8>,
        commit: [u8; 32],
        source_cid: [u8; 32],
        manifest_cid: [u8; 32],
        builder_image_digest: [u8; 32],
        dep_audit_hash: [u8; 32],
    },
    Attest {
        image_id: [u8; 32],
        label: Vec<u8>,
    },
    PublishVerification {
        image_id: [u8; 32],
        verifier_key: [u8; 32],
        sig: Vec<u8>,
        v: [u8; 32],
    },
    IndexInit {
        bucket: u32,
        shard: u32,
    },
    IndexAppend {
        image_id: [u8; 32],
        bucket: u32,
        shard: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one place the mirror's shape is written down. Constructing and
    /// destructuring each variant *by field name* is compiled against the real
    /// enum, so a rename or removal here is a compile error rather than a
    /// silent mismatch, and `expected()` is derived from the same list.
    macro_rules! shape {
        ($($variant:ident { $($field:ident),* $(,)? }),* $(,)?) => {
            fn expected() -> Vec<(String, Vec<&'static str>)> {
                let mut v: Vec<(String, Vec<&'static str>)> = Vec::new();
                $( v.push((snake_case(stringify!($variant)),
                            vec![ $(stringify!($field)),* ])); )*
                v
            }
            #[test]
            fn mirror_shape_compiles() {
                $(
                    let built = Instruction::$variant { $($field: Default::default()),* };
                    match built {
                        Instruction::$variant { $($field),* } => { $( let _ = $field; )* }
                        _ => panic!("{} vanished from the mirror", stringify!($variant)),
                    }
                )*
            }
        };
    }

    fn snake_case(name: &str) -> String {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        }
        out
    }

    shape! {
        Register { image_id, name, version, author_name, description, tags, idl_cid },
        Update { image_id, name, version, author_name, description, tags, idl_cid },
        AttachManifest { image_id, repo_url, commit, source_cid, manifest_cid, builder_image_digest, dep_audit_hash },
        Attest { image_id, label },
        PublishVerification { image_id, verifier_key, sig, v },
        IndexInit { bucket, shard },
        IndexAppend { image_id, bucket, shard },
    }

    /// The guest's dispatcher matches on named fields, so the mirror's variant
    /// order and field names must equal the IDL generated from the guest source.
    #[test]
    fn idl_matches_host_mirror() {
        let idl_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../provenance-idl.json");
        let raw = std::fs::read_to_string(idl_path)
            .unwrap_or_else(|e| panic!("cannot read IDL at {idl_path}: {e}"));
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("IDL is valid JSON");
        let instructions = doc["instructions"].as_array().expect("instructions array");
        let want = expected();

        assert_eq!(
            instructions.len(),
            want.len(),
            "guest has {} instructions, host mirror has {}",
            instructions.len(),
            want.len()
        );

        for (i, got) in instructions.iter().enumerate() {
            let gname = got["name"].as_str().unwrap();
            let (wname, wfields) = &want[i];
            assert_eq!(gname, wname, "variant order differs at index {i}");
            let gfields: Vec<&str> = got["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["name"].as_str().unwrap())
                .collect();
            assert_eq!(gfields, *wfields, "arg names/order differ for {gname}");
        }
    }
}
