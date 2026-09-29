//! Canonical types for the Provenance registry (LP-0023).
//!
//! One schema, one home: the SPEL guest, the `provenance-sdk`, the `lezreg`
//! CLI and `provenance_module` all consume these definitions so product
//! surfaces cannot drift.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::account::Data;
use serde::{Deserialize, Serialize};
use spel_framework_macros::account_type;

pub const SCHEMA_VERSION: u8 = 1;

/// A "not set" content pointer: every `*_cid` field uses all-zero bytes for
/// unset, so optional pointers need no `Option` in the on-chain ABI.
pub const UNSET_CID: [u8; 32] = [0u8; 32];

pub mod limits {
    pub const NAME: usize = 64;
    pub const VERSION: usize = 24;
    pub const AUTHOR_NAME: usize = 48;
    pub const DESCRIPTION: usize = 256;
    pub const TAGS: usize = 8;
    pub const TAG_LEN: usize = 16;
    pub const REPO_URL: usize = 256;
    pub const LABEL: usize = 96;
    pub const INDEX_BUCKET: usize = 512;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaError {
    LabelEmpty,
    NameEmpty,
    NameTooLong,
    VersionTooLong,
    AuthorNameTooLong,
    DescriptionTooLong,
    TooManyTags,
    TagTooLong,
    RepoUrlTooLong,
    LabelTooLong,
    BucketFull,
    Unauthorized,
    IdlCidUnset,
}

impl MetaError {
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::LabelEmpty => 100,
            Self::NameEmpty => 101,
            Self::NameTooLong => 102,
            Self::VersionTooLong => 103,
            Self::AuthorNameTooLong => 104,
            Self::DescriptionTooLong => 105,
            Self::TooManyTags => 106,
            Self::TagTooLong => 107,
            Self::RepoUrlTooLong => 108,
            Self::LabelTooLong => 109,
            Self::BucketFull => 110,
            Self::Unauthorized => 111,
            Self::IdlCidUnset => 112,
        }
    }

    #[must_use]
    pub const fn msg(self) -> &'static str {
        match self {
            Self::LabelEmpty => "label must not be empty",
            Self::NameEmpty => "name must not be empty",
            Self::NameTooLong => "name exceeds 64 bytes",
            Self::VersionTooLong => "version exceeds 24 bytes",
            Self::AuthorNameTooLong => "author_name exceeds 48 bytes",
            Self::DescriptionTooLong => "description exceeds 256 bytes",
            Self::TooManyTags => "more than 8 tags",
            Self::TagTooLong => "tag exceeds 16 bytes",
            Self::RepoUrlTooLong => "repo_url exceeds 256 bytes",
            Self::LabelTooLong => "label exceeds 96 bytes",
            Self::BucketFull => "index bucket full",
            Self::Unauthorized => "signer is not the entry authority",
            Self::IdlCidUnset => "idl_cid must be set",
        }
    }
}

fn bounded(v: &[u8], max: usize, empty_err: Option<MetaError>, err: MetaError) -> Result<(), MetaError> {
    if v.is_empty() && empty_err.is_some() {
        return Err(empty_err.unwrap());
    }
    if v.len() > max {
        return Err(err);
    }
    Ok(())
}

fn validate_meta(
    name: &[u8],
    version: &[u8],
    author_name: &[u8],
    description: &[u8],
    tags: &[Vec<u8>],
) -> Result<(), MetaError> {
    bounded(name, limits::NAME, Some(MetaError::NameEmpty), MetaError::NameTooLong)?;
    bounded(version, limits::VERSION, None, MetaError::VersionTooLong)?;
    bounded(author_name, limits::AUTHOR_NAME, None, MetaError::AuthorNameTooLong)?;
    bounded(description, limits::DESCRIPTION, None, MetaError::DescriptionTooLong)?;
    if tags.len() > limits::TAGS {
        return Err(MetaError::TooManyTags);
    }
    for t in tags {
        bounded(t, limits::TAG_LEN, None, MetaError::TagTooLong)?;
    }
    Ok(())
}

/// Canonical registry record. Lives in the `["entry", program_id]` PDA; only
/// the *first* `register` for a given ImageID can initialize it, which is what
/// makes the claim "first-claim attested" rather than merely asserted.
#[account_type]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Entry {
    pub schema_version: u8,
    /// ImageID bytes of the registered LEZ program.
    pub program_id: [u8; 32],
    /// LEZ account that signed `register`; authorship anchor for `update`.
    pub authority: [u8; 32],
    pub name: Vec<u8>,
    pub version: Vec<u8>,
    pub author_name: Vec<u8>,
    pub description: Vec<u8>,
    pub tags: Vec<Vec<u8>>,
    pub idl_cid: [u8; 32],
    pub source_cid: [u8; 32],
    pub manifest_cid: [u8; 32],
    pub repo_url: Vec<u8>,
    pub commit: [u8; 32],
    pub builder_image_digest: [u8; 32],
    pub dep_audit_hash: [u8; 32],
    pub registered_at: u64,
    pub updated_at: u64,
    pub revision: u32,
}

impl Entry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        program_id: [u8; 32],
        authority: [u8; 32],
        name: Vec<u8>,
        version: Vec<u8>,
        author_name: Vec<u8>,
        description: Vec<u8>,
        tags: Vec<Vec<u8>>,
        idl_cid: [u8; 32],
        now: u64,
    ) -> Result<Self, MetaError> {
        validate_meta(&name, &version, &author_name, &description, &tags)?;
        if idl_cid == UNSET_CID {
            return Err(MetaError::IdlCidUnset);
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            program_id,
            authority,
            name,
            version,
            author_name,
            description,
            tags,
            idl_cid,
            source_cid: UNSET_CID,
            manifest_cid: UNSET_CID,
            repo_url: Vec::new(),
            commit: [0u8; 32],
            builder_image_digest: [0u8; 32],
            dep_audit_hash: [0u8; 32],
            registered_at: now,
            updated_at: now,
            revision: 1,
        })
    }

    /// Author-only metadata edit. Version/idl/description/tags are mutable;
    /// identity fields (program_id, authority, registered_at) are not.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        signer: [u8; 32],
        name: Vec<u8>,
        version: Vec<u8>,
        author_name: Vec<u8>,
        description: Vec<u8>,
        tags: Vec<Vec<u8>>,
        idl_cid: [u8; 32],
        now: u64,
    ) -> Result<(), MetaError> {
        if signer != self.authority {
            return Err(MetaError::Unauthorized);
        }
        validate_meta(&name, &version, &author_name, &description, &tags)?;
        if idl_cid == UNSET_CID {
            return Err(MetaError::IdlCidUnset);
        }
        self.name = name;
        self.version = version;
        self.author_name = author_name;
        self.description = description;
        self.tags = tags;
        self.idl_cid = idl_cid;
        self.updated_at = now;
        self.revision += 1;
        Ok(())
    }

    /// Tier-3 record: the claim that a pinned, reproducible build exists.
    #[allow(clippy::too_many_arguments)]
    pub fn attach_manifest(
        &mut self,
        signer: [u8; 32],
        repo_url: Vec<u8>,
        commit: [u8; 32],
        source_cid: [u8; 32],
        manifest_cid: [u8; 32],
        builder_image_digest: [u8; 32],
        dep_audit_hash: [u8; 32],
        now: u64,
    ) -> Result<(), MetaError> {
        if signer != self.authority {
            return Err(MetaError::Unauthorized);
        }
        bounded(&repo_url, limits::REPO_URL, None, MetaError::RepoUrlTooLong)?;
        self.repo_url = repo_url;
        self.commit = commit;
        self.source_cid = source_cid;
        self.manifest_cid = manifest_cid;
        self.builder_image_digest = builder_image_digest;
        self.dep_audit_hash = dep_audit_hash;
        self.updated_at = now;
        self.revision += 1;
        Ok(())
    }
}

impl TryFrom<&Data> for Entry {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&Entry> for Data {
    fn from(entry: &Entry) -> Self {
        let bytes = borsh::to_vec(entry).expect("Entry serialization should not fail");
        Self::try_from(bytes).expect("Entry should fit into account data")
    }
}

/// Third-party claim on a program ID, in a PDA space physically separate from
/// `Entry` (`["claim3p", program_id, authority]`), so "deployer-class vs
/// third-party" is structural, not a flag an implementation can get wrong.
#[account_type]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ThirdPartyClaim {
    pub schema_version: u8,
    pub program_id: [u8; 32],
    pub authority: [u8; 32],
    pub label: Vec<u8>,
    pub created_at: u64,
}

impl ThirdPartyClaim {
    pub fn new(
        program_id: [u8; 32],
        authority: [u8; 32],
        label: Vec<u8>,
        now: u64,
    ) -> Result<Self, MetaError> {
        bounded(&label, limits::LABEL, Some(MetaError::LabelEmpty), MetaError::LabelTooLong)?;
        Ok(Self { schema_version: SCHEMA_VERSION, program_id, authority, label, created_at: now })
    }
}

impl TryFrom<&Data> for ThirdPartyClaim {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&ThirdPartyClaim> for Data {
    fn from(claim: &ThirdPartyClaim) -> Self {
        let bytes = borsh::to_vec(claim).expect("ThirdPartyClaim serialization should not fail");
        Self::try_from(bytes).expect("ThirdPartyClaim should fit into account data")
    }
}

/// serde for arrays longer than 32 (serde's built-in impls stop at 32;
/// borsh handles them natively).
mod arr64 {
    use serde::{Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(v: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = [u8; 64];

            fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                f.write_str("64 bytes")
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                v.try_into().map_err(|_| E::invalid_length(v.len(), &"64 bytes"))
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut buf: Vec<u8> = Vec::with_capacity(64);
                while let Some(b) = seq.next_element::<u8>()? {
                    buf.push(b);
                }
                buf.try_into().map_err(|v: Vec<u8>| de::Error::invalid_length(v.len(), &"64 bytes"))
            }
        }
        d.deserialize_bytes(V)
    }
}

/// A verifier's statement about one (program, verifier-key) pair, stored at
/// `["vfy", program_id, verifier]`. The transaction signature by `verifier`
/// authorizes the record; `sig` is an off-chain Ed25519 signature over the
/// 32-byte digest `V = hash(program_id || commit || source_cid ||
/// manifest_cid || image_digest)`, re-checked by every reader client-side —
/// the guest never has to understand Ed25519.
#[account_type]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Verification {
    pub schema_version: u8,
    pub program_id: [u8; 32],
    /// LEZ account that signed `publish_verification` (PDA binding).
    pub verifier: [u8; 32],
    /// Ed25519 verifying key the `sig` is checked against.
    pub verifier_key: [u8; 32],
    #[serde(with = "arr64")]
    pub sig: [u8; 64],
    pub v: [u8; 32],
    pub attested_at: u64,
}

impl Verification {
    #[must_use]
    pub const fn new(
        program_id: [u8; 32],
        verifier: [u8; 32],
        verifier_key: [u8; 32],
        sig: [u8; 64],
        v: [u8; 32],
        now: u64,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            program_id,
            verifier,
            verifier_key,
            sig,
            v,
            attested_at: now,
        }
    }
}

impl TryFrom<&Data> for Verification {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&Verification> for Data {
    fn from(v: &Verification) -> Self {
        let bytes = borsh::to_vec(v).expect("Verification serialization should not fail");
        Self::try_from(bytes).expect("Verification should fit into account data")
    }
}

/// Discovery bucket: `program_id[0]` selects the bucket, `shard` grows it.
/// Listable with a bounded batch of account fetches, no indexer required.
#[account_type]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct IndexBucket {
    pub schema_version: u8,
    pub bucket: u32,
    pub shard: u32,
    pub program_ids: Vec<[u8; 32]>,
}

impl IndexBucket {
    #[must_use]
    pub const fn empty(bucket: u32, shard: u32) -> Self {
        Self { schema_version: SCHEMA_VERSION, bucket, shard, program_ids: Vec::new() }
    }

    pub fn push(&mut self, program_id: [u8; 32]) -> Result<(), MetaError> {
        if self.program_ids.len() >= limits::INDEX_BUCKET {
            return Err(MetaError::BucketFull);
        }
        if !self.program_ids.contains(&program_id) {
            self.program_ids.push(program_id);
        }
        Ok(())
    }
}

impl TryFrom<&Data> for IndexBucket {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&IndexBucket> for Data {
    fn from(b: &IndexBucket) -> Self {
        let bytes = borsh::to_vec(b).expect("IndexBucket serialization should not fail");
        Self::try_from(bytes).expect("IndexBucket should fit into account data")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry::new(
            [7u8; 32],
            [1u8; 32],
            b"my program".to_vec(),
            b"0.1.0".to_vec(),
            b"acme".to_vec(),
            Vec::new(),
            vec![b"defi".to_vec()],
            [9u8; 32],
            1234,
        )
        .unwrap()
    }

    #[test]
    fn entry_roundtrips_through_borsh() {
        let e = entry();
        let bytes = borsh::to_vec(&e).unwrap();
        assert_eq!(Entry::try_from_slice(&bytes).unwrap(), e);
        assert!(bytes.len() < 1024, "fixed-shape entry should stay tiny, got {}", bytes.len());
    }

    #[test]
    fn update_requires_authority_and_bumps_revision() {
        let mut e = entry();
        assert_eq!(
            e.update([2u8; 32], b"x".to_vec(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), [9u8; 32], 2222),
            Err(MetaError::Unauthorized)
        );
        e.update([1u8; 32], b"x".to_vec(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), [9u8; 32], 2222)
            .unwrap();
        assert_eq!(e.revision, 2);
        assert_eq!(e.updated_at, 2222);
        assert_eq!(e.registered_at, 1234);
    }

    #[test]
    fn meta_limits_are_enforced() {
        assert_eq!(
            Entry::new([0u8; 32], [0u8; 32], vec![b'a'; 65], Vec::new(), Vec::new(), Vec::new(), Vec::new(), [9u8; 32], 0),
            Err(MetaError::NameTooLong)
        );
        assert_eq!(
            Entry::new([0u8; 32], [0u8; 32], b"ok".to_vec(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), UNSET_CID, 0),
            Err(MetaError::IdlCidUnset)
        );
        let many_tags = vec![vec![b't'; 16]; 9];
        assert_eq!(
            Entry::new([0u8; 32], [0u8; 32], b"ok".to_vec(), Vec::new(), Vec::new(), Vec::new(), many_tags, [9u8; 32], 0),
            Err(MetaError::TooManyTags)
        );
    }

    #[test]
    fn index_bucket_caps_and_dedupes() {
        let mut b = IndexBucket::empty(3, 0);
        b.push([5u8; 32]).unwrap();
        b.push([5u8; 32]).unwrap();
        assert_eq!(b.program_ids.len(), 1);
        let filler = |i: usize| -> [u8; 32] {
            let mut id = [0u8; 32];
            id[..8].copy_from_slice(&(i as u64).to_le_bytes());
            id
        };
        for i in 1..limits::INDEX_BUCKET {
            b.push(filler(i)).unwrap();
        }
        assert_eq!(b.push(filler(999)), Err(MetaError::BucketFull));
    }

    #[test]
    fn verification_and_claim_roundtrip() {
        let v = Verification::new([1u8; 32], [2u8; 32], [3u8; 32], [4u8; 64], [5u8; 32], 42);
        assert_eq!(Verification::try_from_slice(&borsh::to_vec(&v).unwrap()).unwrap(), v);
        let c = ThirdPartyClaim::new([1u8; 32], [2u8; 32], b"not mine".to_vec(), 7).unwrap();
        assert_eq!(ThirdPartyClaim::try_from_slice(&borsh::to_vec(&c).unwrap()).unwrap(), c);
        assert_eq!(
            ThirdPartyClaim::new([1u8; 32], [2u8; 32], Vec::new(), 7),
            Err(MetaError::LabelEmpty)
        );
    }
}
