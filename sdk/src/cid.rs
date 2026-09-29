//! Mapping between CID strings and the registry's 32-byte `*_cid` fields.
//! Parsing accepts any multibase; rendering comes in canonical base32 (`b…`)
//! and base58btc (`z…`) flavors.
//!
//! The on-chain fields store the *multihash digest* (sha2-256 only), not the
//! full CID byte string: a CIDv1 `raw`+sha2-256 CID is 36 bytes (version,
//! codec, hash code, length, digest) and cannot fit a `[u8; 32]`. Round-trip
//! fidelity therefore needs the codec alongside the digest — the manifest
//! carries it (`mimetype`/`blockSize`), and the constructors here take it
//! explicitly.

use cid::{
    Cid,
    multihash::Multihash,
};

use crate::SdkError;

/// Multihash code for sha2-256 (multihash table 0x12).
pub const SHA2_256: u64 = 0x12;

pub mod codec {
    pub const RAW: u64 = 0x55;
    pub const DAG_CBOR: u64 = 0x71;
    pub const DAG_JSON: u64 = 0x0129;
}

/// Extract the sha2-256 digest from a CID string for storage in a registry
/// `[u8; 32]` field.
pub fn cid_to_digest(cid_str: &str) -> Result<[u8; 32], SdkError> {
    let c = Cid::try_from(cid_str).map_err(|e| SdkError::BadCid(cid_str.to_string(), e.to_string()))?;
    let mh = c.hash();
    if mh.code() != SHA2_256 {
        return Err(SdkError::UnsupportedHash(mh.code()));
    }
    mh.digest()
        .try_into()
        .map_err(|_| SdkError::BadDigestLen(mh.digest().len()))
}

/// Rebuild the CID string from a stored digest plus its codec.
/// Canonical `CIDv1 + base32` rendering (`baf…`); parsers here accept any
/// multibase, so whichever base the live gateway prints round-trips through
/// [`cid_to_digest`] unchanged.
pub fn digest_to_cid(codec: u64, digest: &[u8; 32]) -> Result<String, SdkError> {
    let mh = Multihash::wrap(SHA2_256, digest).map_err(|e| SdkError::Other(e.to_string()))?;
    Ok(Cid::new_v1(codec, mh).to_string())
}

/// Same CID rendered as `z…` base58btc — the form Logos Storage's own
/// documentation uses (its examples decode to CIDv1 + sha2-256 + codecs in
/// the 0xcd__ custom namespace, multibase prefix `z` = base58btc). The exact
/// base the live gateway returns is still a probe item for when Basecamp
/// runs; parsing is base-agnostic either way.
pub fn digest_to_cid_base58btc(codec: u64, digest: &[u8; 32]) -> Result<String, SdkError> {
    let mh = Multihash::wrap(SHA2_256, digest).map_err(|e| SdkError::Other(e.to_string()))?;
    let c = Cid::new_v1(codec, mh);
    c.to_string_of_base(cid::multibase::Base::Base58Btc)
        .map_err(|e| SdkError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest as _, Sha256};

    fn digest_of(payload: &[u8]) -> [u8; 32] {
        Sha256::digest(payload).into()
    }

    #[test]
    fn cid_roundtrips_through_digest() {
        for codec in [codec::RAW, codec::DAG_CBOR, codec::DAG_JSON] {
            let digest = digest_of(b"provenance fixture");
            let cid = digest_to_cid(codec, &digest).unwrap();
            assert!(cid.starts_with('b'), "canonical render is base32: {cid}");
            assert_eq!(cid_to_digest(&cid).unwrap(), digest);
            let z = digest_to_cid_base58btc(codec, &digest).unwrap();
            assert!(z.starts_with('z'), "base58btc multibase prefix: {z}");
            assert_eq!(cid_to_digest(&z).unwrap(), digest);
        }
    }

    #[test]
    fn known_vectors_match_python_computed() {
        // CIDv1 raw over the empty payload; both renderings computed
        // independently in python (rfc4648-lower and base58btc alphabets).
        let empty = digest_of(b"");
        assert_eq!(
            digest_to_cid(codec::RAW, &empty).unwrap(),
            "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"
        );
        assert_eq!(
            digest_to_cid_base58btc(codec::RAW, &empty).unwrap(),
            "zb2rhmy65F3REf8SZp7De11gxtECBGgUKaLdiDj7MCGCHxbDW"
        );
    }

    #[test]
    fn non_sha256_and_garbage_are_rejected() {
        let digest = digest_of(b"x");
        let weird = Cid::new_v1(codec::RAW, Multihash::wrap(0x1200, &digest).unwrap());
        assert!(matches!(
            cid_to_digest(&weird.to_string()),
            Err(SdkError::UnsupportedHash(0x1200))
        ));
        assert!(matches!(cid_to_digest("not-a-cid"), Err(SdkError::BadCid(..))));
    }
}
