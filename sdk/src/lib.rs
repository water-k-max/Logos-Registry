//! Client SDK for the Provenance registry (LP-0023).
//!
//! Storage-agnostic by design: content goes through the [`storage::Storage`]
//! trait (Codex REST impl today, FFI later), records come off chain through
//! [`chain::Sequencer`], and every reader re-checks verifier signatures locally
//! through [`verify`] — the chain only stores bound records, trust comes from the
//! reader's own [`verify::TrustStore`].

pub mod chain;
pub mod cid;
pub mod storage;
pub mod verify;

pub use provenance_core::{Entry, IndexBucket, ThirdPartyClaim, Verification, UNSET_CID};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("invalid CID `{0}`: {1}")]
    BadCid(String, String),
    #[error("unsupported CID content hash (want sha2-256), got code 0x{0:x}")]
    UnsupportedHash(u64),
    #[error("digest length {0} != 32")]
    BadDigestLen(usize),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("chain error: {0}")]
    Chain(String),
    #[error("{0}")]
    Other(String),
}
