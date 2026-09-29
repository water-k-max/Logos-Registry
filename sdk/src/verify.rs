//! Attestation digest + Ed25519 verification + reader trust store.
//!
//! Mirrors the `Verification` contract in `provenance_core`: the record
//! carries `V = sha256(program_id || commit || source_cid || manifest_cid ||
//! builder_image_digest)` and an Ed25519 signature over `V` by
//! `verifier_key`. The guest stores the binding; readers decide whom to
//! trust via [`TrustStore`] — nothing on-chain is "verified" on its own
//! authority.

use std::{collections::HashMap, fs, path::Path};

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use provenance_core::Verification;

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("v field is not the canonical digest of the attested materials")]
    DigestMismatch,
    #[error("invalid ed25519 verifying key: {0}")]
    BadKey(String),
    #[error("bad signature: {0}")]
    BadSig(String),
    #[error("verifier key {0} is not in the trust store")]
    Untrusted(String),
}

/// The 32-byte `V` a verifier signs.
pub fn attestation_digest(
    program_id: &[u8; 32],
    commit: &[u8; 32],
    source_cid: &[u8; 32],
    manifest_cid: &[u8; 32],
    builder_image_digest: &[u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(program_id);
    h.update(commit);
    h.update(source_cid);
    h.update(manifest_cid);
    h.update(builder_image_digest);
    h.finalize().into()
}

/// Full client-side check for one record given the attested materials:
/// `v` matches the digest of the materials, and `sig` is a valid Ed25519
/// signature over `v` by `verifier_key`.
pub fn verify_attestation(
    record: &Verification,
    commit: &[u8; 32],
    source_cid: &[u8; 32],
    manifest_cid: &[u8; 32],
    builder_image_digest: &[u8; 32],
) -> Result<(), VerifyError> {
    let v = attestation_digest(&record.program_id, commit, source_cid, manifest_cid, builder_image_digest);
    if v != record.v {
        return Err(VerifyError::DigestMismatch);
    }
    let key = VerifyingKey::from_bytes(&record.verifier_key).map_err(|e| VerifyError::BadKey(e.to_string()))?;
    let sig = Signature::from_slice(&record.sig).map_err(|e| VerifyError::BadSig(e.to_string()))?;
    key.verify_strict(&v, &sig).map_err(|e| VerifyError::BadSig(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedVerifier {
    pub name: String,
    /// Hex-encoded 32-byte Ed25519 verifying key.
    pub pubkey_hex: String,
    #[serde(default)]
    pub note: String,
}

/// Reader-local allow-list of verifier keys, loaded from JSON:
/// `{"verifiers":[{"name":"…","pubkey_hex":"…","note":"…"}]}`.
#[derive(Debug, Default)]
pub struct TrustStore {
    by_key: HashMap<[u8; 32], TrustedVerifier>,
}

impl TrustStore {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_json(&raw)
    }

    pub fn from_json(raw: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct File {
            verifiers: Vec<TrustedVerifier>,
        }
        let f: File = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        let mut store = Self::default();
        for v in f.verifiers {
            let bytes = hex::decode(&v.pubkey_hex).map_err(|e| format!("{}: {e}", v.name))?;
            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| format!("{}: pubkey must be 32 bytes", v.name))?;
            store.by_key.insert(key, v);
        }
        Ok(store)
    }

    #[must_use]
    pub fn is_trusted(&self, pubkey: &[u8; 32]) -> bool {
        self.by_key.contains_key(pubkey)
    }

    #[must_use]
    pub fn get(&self, pubkey: &[u8; 32]) -> Option<&TrustedVerifier> {
        self.by_key.get(pubkey)
    }

    /// `verify_attestation` plus the trust-store gate.
    pub fn verify_trusted(
        &self,
        record: &Verification,
        commit: &[u8; 32],
        source_cid: &[u8; 32],
        manifest_cid: &[u8; 32],
        builder_image_digest: &[u8; 32],
    ) -> Result<TrustedVerifier, VerifyError> {
        verify_attestation(record, commit, source_cid, manifest_cid, builder_image_digest)?;
        self.get(&record.verifier_key).cloned().ok_or_else(|| VerifyError::Untrusted(hex::encode(record.verifier_key)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};

    const COMMIT: [u8; 32] = [8u8; 32];
    const SOURCE: [u8; 32] = [9u8; 32];
    const MANIFEST: [u8; 32] = [10u8; 32];
    const IMAGE: [u8; 32] = [11u8; 32];

    fn fixture() -> Verification {
        let signing = SigningKey::from_bytes(&[3u8; 32]);
        let vk = signing.verifying_key().to_bytes();
        let program_id = [7u8; 32];
        let v = attestation_digest(&program_id, &COMMIT, &SOURCE, &MANIFEST, &IMAGE);
        let sig = signing.sign(&v).to_bytes();
        Verification::new(program_id, [1u8; 32], vk, sig, v, 100)
    }

    #[test]
    fn honest_attestation_verifies() {
        let record = fixture();
        verify_attestation(&record, &COMMIT, &SOURCE, &MANIFEST, &IMAGE).unwrap();
    }

    #[test]
    fn digest_binds_field_order() {
        let a = attestation_digest(&[1u8; 32], &COMMIT, &SOURCE, &MANIFEST, &IMAGE);
        let b = attestation_digest(&[1u8; 32], &SOURCE, &COMMIT, &MANIFEST, &IMAGE);
        assert_ne!(a, b, "swapping fields must change V");
    }

    #[test]
    fn tampered_materials_or_sig_are_rejected() {
        let record = fixture();
        assert!(matches!(
            verify_attestation(&record, &[0u8; 32], &SOURCE, &MANIFEST, &IMAGE),
            Err(VerifyError::DigestMismatch)
        ));
        let mut bad = record.clone();
        bad.sig[0] ^= 0x01;
        assert!(matches!(
            verify_attestation(&bad, &COMMIT, &SOURCE, &MANIFEST, &IMAGE),
            Err(VerifyError::BadSig(_))
        ));
    }

    #[test]
    fn trust_store_gates_unknown_keys() {
        let record = fixture();
        let key_hex = hex::encode(record.verifier_key);
        let json = format!(
            "{{\"verifiers\":[{{\"name\":\"acme-auditors\",\"pubkey_hex\":\"{key_hex}\",\"note\":\"demo\"}}]}}"
        );
        let store = TrustStore::from_json(&json).unwrap();
        let v = store
            .verify_trusted(&record, &COMMIT, &SOURCE, &MANIFEST, &IMAGE)
            .unwrap();
        assert_eq!(v.name, "acme-auditors");

        let empty = TrustStore::default();
        assert!(matches!(
            empty.verify_trusted(&record, &COMMIT, &SOURCE, &MANIFEST, &IMAGE),
            Err(VerifyError::Untrusted(_))
        ));
    }
}
