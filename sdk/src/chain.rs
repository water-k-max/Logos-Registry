//! Chain read path: sequencer JSON-RPC, registry addresses, write confirmation.
//!
//! Two rules this module will not bend:
//!
//! * **Addresses come from upstream.** Every PDA here goes through
//!   [`spel_framework_core::pda::compute_pda`], the same function the state machine
//!   runs. A hand-rolled replacement keeps returning a well-formed address after the
//!   seed layout changes on chain — it just points at nothing, silently.
//! * **A transaction hash is not a result.** The sequencer exposes no `simulate` and
//!   no status field, and `getTransaction` answers `null` for a transaction that was
//!   included but failed, so the only evidence a write landed is the target account's
//!   own bytes changing: hash with [`Account::data_sha256`] before submitting, then
//!   [`Sequencer::wait_for_data_change`].
//!
//! ## Which program namespace registry accounts live in
//!
//! The state machine derives a PDA claim against the **callee** program id and then
//! *sets* `program_owner` to that same id
//! (`lee/state_machine/src/validated_state_diff/mod.rs:236`, LEZ `v0.2.4` / `47eba25`):
//!
//! ```text
//! let pda = AccountId::for_public_pda(&chained_call.program_id, &seed);
//! ensure!(account_id == pda, InvalidProgramBehaviorError::MismatchedPdaClaim { .. });
//! post.account_mut().program_owner = chained_call.program_id;
//! ```
//!
//! So an entry for *someone else's* program is still owned by the registry, and its
//! address is derived from the registry's ImageID with the subject ImageID used only
//! as seed data. Every derivation below therefore takes `registry` explicitly.
//! Getting this wrong is invisible while the registry holds only its own entry: both
//! readings collapse to the same address when subject == registry.

use std::{
    time::{Duration, Instant},
};

use borsh::BorshDeserialize;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

// Both types appear in this module's public signatures, so callers have to be able to name them.
// Re-exported rather than made callers depend on `lee_core` directly: the SDK picks that
// dependency's version, not the integrator.
pub use lee_core::account::AccountId;
pub use lee_core::program::ProgramId;

use provenance_core::{Entry, IndexBucket, SCHEMA_VERSION, ThirdPartyClaim, Verification};

use crate::SdkError;

/// Testnet sequencer. Override with `LEZ_RPC_URL`.
pub const DEFAULT_RPC_URL: &str = "https://testnet.lez.logos.co/";

/// Poll interval and write-confirmation timeout for [`Sequencer::wait_for_data_change`].
/// Measured on the testnet (`scripts/rpc_cadence.sh`, 2026-09-28): the tip moved one
/// block in 89 s of sampling, and 437 blocks in the ~6 h between the `register` tx
/// (block 28803) and this reading, so a block is ~50-90 s when the zone is quiet.
/// 8 s polling keeps latency low without hammering; 300 s spans ~4-6 blocks.
pub const DEFAULT_POLL: Duration = Duration::from_millis(8_000);
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

const REGISTRY_IMAGE_ID_HEX: &str =
    "7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6";

/// ImageID of the deployed registry (testnet), decoded at compile time from
/// [`REGISTRY_IMAGE_ID_HEX`] so the two cannot drift apart.
pub const REGISTRY_IMAGE_ID: [u8; 32] = image_id(REGISTRY_IMAGE_ID_HEX);

const fn image_id(hex: &str) -> [u8; 32] {
    let b = hex.as_bytes();
    assert!(b.len() == 64, "ImageID must be 64 hex chars");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("ImageID contains a non-hex character"),
    }
}

/// ImageID bytes -> the chain's `ProgramId` (8 little-endian u32 limbs), the same
/// layout `program_owner` is reported in.
#[must_use]
pub fn program_id(image: &[u8; 32]) -> ProgramId {
    let mut limbs = [0u32; 8];
    for (i, chunk) in image.chunks(4).enumerate() {
        limbs[i] = u32::from_le_bytes(chunk.try_into().expect("32 bytes -> 8 limbs"));
    }
    limbs
}

/// Hex (optionally `0x`-prefixed) -> 32-byte ImageID.
pub fn image_id_from_hex(raw: &str) -> Result<[u8; 32], SdkError> {
    let s = raw.strip_prefix("0x").unwrap_or(raw);
    let bytes = hex::decode(s).map_err(|e| SdkError::Other(format!("bad ImageID hex: {e}")))?;
    bytes.try_into().map_err(|v: Vec<u8>| SdkError::BadDigestLen(v.len()))
}

/// Refuse to interpret a record this SDK has never seen a schema for.
fn expect_schema(version: u8, what: &str) -> Result<(), SdkError> {
    if version != SCHEMA_VERSION {
        return Err(SdkError::Chain(format!(
            "{what} is schema {version}, this SDK reads {SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

/// The schema byte is the first field of every record, so it is checked *before* borsh
/// runs: against a future layout a blind decode would either fail as an opaque error or,
/// if the field order merely swapped, succeed with the wrong values in the wrong fields.
fn expect_schema_at(data: &[u8], what: &str) -> Result<(), SdkError> {
    let version = data
        .first()
        .copied()
        .ok_or_else(|| SdkError::Chain(format!("{what} account has no data")))?;
    expect_schema(version, what)
}

/// Seeds mirror `methods/guest/src/bin/provenance.rs` exactly:
/// `pda = [literal("entry"), arg("image_id")]` and friends.
#[must_use]
pub fn entry_pda(registry: &[u8; 32], image_id: &[u8; 32]) -> AccountId {
    use spel_framework_core::pda::{compute_pda, seed_from_str};
    compute_pda(&program_id(registry), &[&seed_from_str("entry"), image_id])
}

/// `pda = [literal("claim3p"), arg("image_id"), account("authority")]`
#[must_use]
pub fn claim_pda(registry: &[u8; 32], image_id: &[u8; 32], authority: &AccountId) -> AccountId {
    use spel_framework_core::pda::{compute_pda, seed_from_str};
    compute_pda(
        &program_id(registry),
        &[&seed_from_str("claim3p"), image_id, authority.value()],
    )
}

/// `pda = [literal("vfy"), arg("image_id"), account("verifier")]`
#[must_use]
pub fn verification_pda(registry: &[u8; 32], image_id: &[u8; 32], verifier: &AccountId) -> AccountId {
    use spel_framework_core::pda::{compute_pda, seed_from_str};
    compute_pda(
        &program_id(registry),
        &[&seed_from_str("vfy"), image_id, verifier.value()],
    )
}

/// `pda = [literal("index"), arg("bucket"), arg("shard")]`. Not image-scoped: one
/// bucket space per registry, selected by the registered ImageID's first byte.
#[must_use]
pub fn index_pda(registry: &[u8; 32], bucket: u32, shard: u32) -> AccountId {
    use spel_framework_core::pda::{compute_pda, seed_from_str, ToSeed};
    compute_pda(
        &program_id(registry),
        &[&seed_from_str("index"), &bucket.to_seed(), &shard.to_seed()],
    )
}

/// One account as the sequencer reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Owning program, as 8 little-endian u32 limbs — the ImageID limbs of the owner.
    #[serde(default)]
    pub program_owner: ProgramId,
    #[serde(default)]
    pub balance: u64,
    #[serde(default)]
    pub data: Vec<u8>,
    #[serde(default)]
    pub nonce: u64,
}

impl Account {
    /// Measured: a nonexistent account is **not** `null` and not an error. The gateway
    /// answers `{"program_owner":[0;8],"balance":0,"data":[],"nonce":0}`, so "missing"
    /// has to be recognised from its shape or every read looks like an empty record.
    #[must_use]
    pub fn is_missing(&self) -> bool {
        self.data.is_empty() && self.program_owner == [0u32; 8]
    }

    /// The `program_owner` limbs of the registry program, i.e. what an account created
    /// by `register`/`attest`/… must be owned by.
    #[must_use]
    pub fn owned_by(&self, registry_image: &[u8; 32]) -> bool {
        self.program_owner == program_id(registry_image)
    }

    #[must_use]
    pub fn data_sha256(&self) -> [u8; 32] {
        Sha256::digest(&self.data).into()
    }

    /// Borsh-decode the account body. Trailing bytes are an error, not padding to
    /// ignore: a schema bump must fail loudly here rather than half-parse.
    pub fn decode<T: BorshDeserialize>(&self) -> Result<T, SdkError> {
        T::try_from_slice(&self.data)
            .map_err(|e| SdkError::Other(format!("borsh decode of {} bytes: {e}", self.data.len())))
    }
}

/// `getTransaction` result: `[base64(transaction), block_id]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub block_id: u64,
    pub bytes: Vec<u8>,
}

pub struct Sequencer {
    agent: ureq::Agent,
    url: String,
    registry: [u8; 32],
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Sequencer {
    /// `url` defaults from env `LEZ_RPC_URL`, else [`DEFAULT_RPC_URL`]. The registry
    /// ImageID is the pinned testnet deployment; use [`Sequencer::with_registry`] for
    /// another one.
    #[must_use]
    pub fn new(url: Option<String>) -> Self {
        Self::with_timeout(url, Duration::from_secs(60))
    }

    /// Same, with the per-request timeout callers may want to bound (`new` uses 60 s,
    /// and `call` retries transport failures 3 times, so the worst case is 3x this).
    #[must_use]
    pub fn with_timeout(url: Option<String>, timeout: Duration) -> Self {
        let url = url
            .or_else(|| std::env::var("LEZ_RPC_URL").ok())
            .unwrap_or_else(|| DEFAULT_RPC_URL.to_string());
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(timeout)
                .timeout_connect(timeout)
                .try_proxy_from_env(true)
                .build(),
            url: url.trim_end_matches('/').to_string(),
            registry: REGISTRY_IMAGE_ID,
        }
    }

    #[must_use]
    pub fn with_registry(mut self, registry_image_id: [u8; 32]) -> Self {
        self.registry = registry_image_id;
        self
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The registry whose namespace every address here is derived in.
    #[must_use]
    pub fn registry(&self) -> &[u8; 32] {
        &self.registry
    }

    #[must_use]
    pub fn entry_account(&self, image_id: &[u8; 32]) -> Result<Account, SdkError> {
        self.account(&entry_pda(&self.registry, image_id))
    }

    fn call(&self, method: &str, params: Value) -> Result<Value, SdkError> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        let mut transport = String::new();
        for attempt in 0..3 {
            let sent = self
                .agent
                .post(&self.url)
                .set("content-type", "application/json")
                .send_string(&body);
            match sent {
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.into_string().map_err(|e| {
                        SdkError::Chain(format!("{method}: reading body: {e}"))
                    })?;
                    if !(200..300).contains(&status) {
                        return Err(SdkError::Chain(format!("{method}: HTTP {status}: {text}")));
                    }
                    let v: Value = serde_json::from_str(&text).map_err(|e| {
                        SdkError::Chain(format!("{method}: bad JSON-RPC reply `{text}`: {e}"))
                    })?;
                    if let Some(err) = v.get("error") {
                        return Err(SdkError::Chain(format!(
                            "{method}: {}",
                            err.get("message").and_then(Value::as_str).unwrap_or(&err.to_string())
                        )));
                    }
                    return v
                        .get("result")
                        .cloned()
                        .ok_or_else(|| SdkError::Chain(format!("{method}: reply had no result")));
                }
                Err(e) => {
                    transport = e.to_string();
                    std::thread::sleep(Duration::from_millis(400 * (attempt + 1)));
                }
            }
        }
        Err(SdkError::Chain(format!("{method}: {transport}")))
    }

    /// Blank account (see [`Account::is_missing`]) when the chain has no such account.
    pub fn account(&self, id: &AccountId) -> Result<Account, SdkError> {
        let result = self.call("getAccount", json!({"account_id": id.to_string()}))?;
        serde_json::from_value(result)
            .map_err(|e| SdkError::Chain(format!("getAccount({id}): unparsable reply: {e}")))
    }

    pub fn last_block_id(&self) -> Result<u64, SdkError> {
        let result = self.call("getLastBlockId", json!({}))?;
        result
            .as_u64()
            .ok_or_else(|| SdkError::Chain(format!("getLastBlockId: not a number: {result}")))
    }

    /// `None` means the sequencer does not know this hash — which covers both "never
    /// submitted" and "submitted, executed, failed" (measured: unknown hash ->
    /// `result: null`). It is *not* proof of anything about your write.
    pub fn transaction(&self, tx_hash: &str) -> Result<Option<Transaction>, SdkError> {
        let result = self.call("getTransaction", json!({"tx_hash": tx_hash}))?;
        if result.is_null() {
            return Ok(None);
        }
        let pair = result
            .as_array()
            .ok_or_else(|| SdkError::Chain(format!("getTransaction: expected [tx, block], got {result}")))?;
        if pair.len() != 2 {
            return Err(SdkError::Chain(format!("getTransaction: expected 2 fields, got {}", pair.len())));
        }
        let raw = pair[0]
            .as_str()
            .ok_or_else(|| SdkError::Chain("getTransaction: tx not base64 string".into()))?;
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .map_err(|e| SdkError::Chain(format!("getTransaction: base64: {e}")))?;
        let block_id = pair[1]
            .as_u64()
            .ok_or_else(|| SdkError::Chain("getTransaction: block id not a number".into()))?;
        Ok(Some(Transaction { block_id, bytes }))
    }

    /// The canonical entry for one ImageID, or `None` when nobody has claimed it —
    /// which is the registry's most important answer, so it is not an error.
    ///
    /// Rejects an account that is not owned by the registry, whose stored `program_id`
    /// is not the queried ImageID, or whose schema this SDK cannot read.
    pub fn entry(&self, image_id: &[u8; 32]) -> Result<Option<Entry>, SdkError> {
        let acc = self.entry_account(image_id)?;
        if acc.is_missing() {
            return Ok(None);
        }
        self.expect_registry_account(&acc, "entry")?;
        expect_schema_at(&acc.data, "entry")?;
        let entry: Entry = acc.decode()?;
        if entry.program_id != *image_id {
            return Err(SdkError::Chain(format!(
                "entry at {} stores program_id {} instead of {}",
                entry_pda(&self.registry, image_id),
                hex::encode(entry.program_id),
                hex::encode(image_id)
            )));
        }
        Ok(Some(entry))
    }

    pub fn claim(&self, image_id: &[u8; 32], authority: &AccountId) -> Result<Option<ThirdPartyClaim>, SdkError> {
        let acc = self.account(&claim_pda(&self.registry, image_id, authority))?;
        if acc.is_missing() {
            return Ok(None);
        }
        self.expect_registry_account(&acc, "claim")?;
        expect_schema_at(&acc.data, "claim")?;
        let claim: ThirdPartyClaim = acc.decode()?;
        if claim.program_id != *image_id || claim.authority != *authority.value() {
            return Err(SdkError::Chain(
                "claim account does not bind the queried (program, authority)".into(),
            ));
        }
        Ok(Some(claim))
    }

    pub fn verification(&self, image_id: &[u8; 32], verifier: &AccountId) -> Result<Option<Verification>, SdkError> {
        let acc = self.account(&verification_pda(&self.registry, image_id, verifier))?;
        if acc.is_missing() {
            return Ok(None);
        }
        self.expect_registry_account(&acc, "verification")?;
        expect_schema_at(&acc.data, "verification")?;
        let record: Verification = acc.decode()?;
        if record.program_id != *image_id || record.verifier != *verifier.value() {
            return Err(SdkError::Chain(
                "verification account does not bind the queried (program, verifier)".into(),
            ));
        }
        Ok(Some(record))
    }

    pub fn bucket(&self, bucket: u32, shard: u32) -> Result<Option<IndexBucket>, SdkError> {
        let acc = self.account(&index_pda(&self.registry, bucket, shard))?;
        if acc.is_missing() {
            return Ok(None);
        }
        self.expect_registry_account(&acc, "index bucket")?;
        expect_schema_at(&acc.data, "index bucket")?;
        let b: IndexBucket = acc.decode()?;
        if b.bucket != bucket || b.shard != shard {
            return Err(SdkError::Chain(format!(
                "index bucket account says ({}, {})",
                b.bucket, b.shard
            )));
        }
        Ok(Some(b))
    }

    fn expect_registry_account(&self, acc: &Account, what: &str) -> Result<(), SdkError> {
        if !acc.owned_by(&self.registry) {
            return Err(SdkError::Chain(format!(
                "{what} account is owned by {}, not by the registry {}",
                hex::encode(acc.program_owner.map(u32::to_le_bytes).concat()),
                hex::encode(self.registry)
            )));
        }
        Ok(())
    }

    /// Hash of an account's data right now — the "before" half of a write.
    pub fn snapshot(&self, id: &AccountId) -> Result<[u8; 32], SdkError> {
        Ok(self.account(id)?.data_sha256())
    }

    /// Block until the account's bytes differ from `before`, then hand back the new
    /// account. This is how a submission is confirmed on a network that reports no
    /// execution status; a timeout means "unknown", not "rejected".
    pub fn wait_for_data_change(
        &self,
        id: &AccountId,
        before: [u8; 32],
        timeout: Duration,
    ) -> Result<Account, SdkError> {
        self.wait_for_data_change_with(id, before, timeout, DEFAULT_POLL)
    }

    pub fn wait_for_data_change_with(
        &self,
        id: &AccountId,
        before: [u8; 32],
        timeout: Duration,
        poll: Duration,
    ) -> Result<Account, SdkError> {
        let deadline = Instant::now() + timeout;
        loop {
            let acc = self.account(id)?;
            if acc.data_sha256() != before {
                return Ok(acc);
            }
            if Instant::now() >= deadline {
                return Err(SdkError::Chain(format!(
                    "account {id} unchanged after {}s; the write is unconfirmed, not proven failed",
                    timeout.as_secs()
                )));
            }
            std::thread::sleep(poll);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const IMAGE: &str = "7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6";
    /// The live canonical entry for the registry's own ImageID.
    const ENTRY_ACCOUNT: &str = "Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ";
    const AUTHORITY_BASE58: &str = "8tWS2X8e59Q4FYUUExxxFzpNjbQirzkFjDjYSukVzsqe";
    const AUTHORITY_HEX: &str = "7533b541c4f6dcd8c4a06047af57f7cf9b5c672a7a50f53088bc34d93499ed15";

    /// 395 bytes fetched from the live entry PDA after `register` landed.
    const LIVE_ENTRY: &[u8; 395] = include_bytes!("../tests/fixtures/live-entry-395.bin");
    /// The verbatim `getAccount` JSON-RPC envelope for that PDA.
    const LIVE_ENVELOPE: &str = include_str!("../tests/fixtures/live-get-account.json");

    fn image() -> [u8; 32] {
        image_id_from_hex(IMAGE).unwrap()
    }

    /// Unwrap the envelope exactly the way [`Sequencer::call`] does, then parse the
    /// account the way [`Sequencer::account`] does.
    fn live_account() -> Account {
        let envelope: Value = serde_json::from_str(LIVE_ENVELOPE).unwrap();
        serde_json::from_value(envelope["result"].clone()).unwrap()
    }

    #[test]
    fn pinned_registry_constant_is_the_deployed_image_id() {
        assert_eq!(REGISTRY_IMAGE_ID, image());
        assert_eq!(hex::encode(REGISTRY_IMAGE_ID), REGISTRY_IMAGE_ID_HEX);
    }

    #[test]
    fn program_id_is_image_id_as_little_endian_limbs() {
        // measured: the live entry reports program_owner [3544203387, 1328923147, …]
        assert_eq!(program_id(&image())[..2], [3544203387, 1328923147]);
    }

    /// The address test that matters: derived here, deployed on chain.
    #[test]
    fn derived_entry_pda_is_the_live_entry_account() {
        assert_eq!(entry_pda(&REGISTRY_IMAGE_ID, &image()).to_string(), ENTRY_ACCOUNT);
    }

    /// Self-registration hides a namespace bug: subject == registry, so the wrong
    /// derivation (pid from the *subject* ImageID) still lands on the live address.
    /// Pin that this SDK's API cannot express that confusion by taking both ids.
    #[test]
    fn a_third_party_entry_is_not_in_the_subjects_own_namespace() {
        let subject = [0xeeu8; 32];
        let right = entry_pda(&REGISTRY_IMAGE_ID, &subject);
        let wrong = entry_pda(&subject, &subject);
        assert_ne!(right, wrong);
        // printed so `tools/lezreg` (a separate binary) can be cross-checked against it
        println!("third-party entry pda  {right}");
        // and the registry's own entry must be unaffected by the subject choice
        assert_eq!(entry_pda(&REGISTRY_IMAGE_ID, &image()).to_string(), ENTRY_ACCOUNT);
    }

    #[test]
    fn other_pda_spaces_are_distinct_and_stable() {
        let authority = AccountId::from_str(AUTHORITY_BASE58).unwrap();
        assert_eq!(authority.value(), &image_id_from_hex(AUTHORITY_HEX).unwrap());
        let i = image();
        assert_ne!(claim_pda(&REGISTRY_IMAGE_ID, &i, &authority), entry_pda(&REGISTRY_IMAGE_ID, &i));
        assert_ne!(
            verification_pda(&REGISTRY_IMAGE_ID, &i, &authority),
            claim_pda(&REGISTRY_IMAGE_ID, &i, &authority)
        );
        assert_ne!(index_pda(&REGISTRY_IMAGE_ID, 0x7b, 0), index_pda(&REGISTRY_IMAGE_ID, 0x7b, 1));
        assert_eq!(index_pda(&REGISTRY_IMAGE_ID, 7, 0), index_pda(&REGISTRY_IMAGE_ID, 7, 0));
    }

    #[test]
    fn the_live_entry_decodes_and_binds_what_it_claims() {
        let account = live_account();
        assert_eq!(account.data.len(), 395);
        assert_eq!(account.data, LIVE_ENTRY.as_slice());
        assert!(!account.is_missing());
        assert!(account.owned_by(&REGISTRY_IMAGE_ID));

        let entry: Entry = account.decode().unwrap();
        expect_schema(entry.schema_version, "entry").unwrap();
        assert!(expect_schema_at(&[2, 0, 0, 0], "entry").is_err(), "schema 2 is not this SDK's schema");
        assert_eq!(entry.schema_version, 1);
        assert_eq!(entry.program_id, image());
        assert_eq!(entry.authority, image_id_from_hex(AUTHORITY_HEX).unwrap());
        assert_eq!(entry.name, b"Provenance");
        assert_eq!(entry.version, b"0.1.0");
        assert_eq!(entry.author_name, b"Strategic Edge");
        assert_eq!(entry.description, b"LEZ program registry");
        assert_eq!(entry.tags, vec![b"registry".to_vec(), b"provenance".to_vec(), b"verified-builds".to_vec()]);
        assert_eq!(
            hex::encode(entry.idl_cid),
            "ef6635b714c70b0f7c08e6649fbb60b04b82317519acfc701c372f2a712d382f"
        );
        assert_eq!(entry.source_cid, provenance_core::UNSET_CID);
        assert_eq!(entry.manifest_cid, provenance_core::UNSET_CID);
        assert_eq!(entry.repo_url, Vec::<u8>::new());
        assert_eq!(entry.registered_at, 1_790_606_384_850);
        assert_eq!(entry.updated_at, 1_790_606_384_850);
        assert_eq!(entry.revision, 1);
        // the hash the duplicate-claim experiment recorded for this exact account
        assert_eq!(
            hex::encode(account.data_sha256()),
            "874aeba96fcdb9f8caf66a7d851835999cd9989ee25278fe5876bf396b5fb968"
        );
    }

    #[test]
    fn a_missing_account_is_recognised_not_decoded() {
        let blank: Account = serde_json::from_value(json!({
            "program_owner": [0, 0, 0, 0, 0, 0, 0, 0], "balance": 0, "data": [], "nonce": 0
        }))
        .unwrap();
        assert!(blank.is_missing());
        assert_eq!(blank, Account::default());
        assert!(blank.decode::<Entry>().is_err());
    }

    #[test]
    fn trailing_or_truncated_data_is_a_decode_error() {
        let mut account = Account { data: LIVE_ENTRY.to_vec(), ..Default::default() };
        assert!(account.decode::<Entry>().is_ok());
        account.data.push(0);
        assert!(account.decode::<Entry>().is_err(), "a schema bump must not half-parse");
        account.data = LIVE_ENTRY[..390].to_vec();
        assert!(account.decode::<Entry>().is_err());
    }

    #[test]
    fn an_account_owned_by_another_program_is_rejected() {
        let seq = Sequencer::new(Some("http://127.0.0.1:1".into()));
        let mut account = live_account();
        account.program_owner = [1, 0, 0, 0, 0, 0, 0, 0];
        let err = seq.expect_registry_account(&account, "entry").unwrap_err();
        assert!(err.to_string().contains("not by the registry"), "{err}");
    }

    #[test]
    fn url_and_registry_defaults() {
        std::env::remove_var("LEZ_RPC_URL");
        let seq = Sequencer::new(None);
        assert_eq!(seq.url(), "https://testnet.lez.logos.co");
        assert_eq!(seq.registry(), &REGISTRY_IMAGE_ID);
        let seq = Sequencer::new(None).with_registry([9u8; 32]);
        assert_eq!(seq.registry(), &[9u8; 32]);
    }

    #[test]
    fn transport_failure_is_reported_not_panicked() {
        // 192.0.2.1 (TEST-NET-1) is unrouteable by design, so this measures the real
        // failure path: 2 s to give up per attempt, 3 attempts, and an error value —
        // not a hang and not a panic. A client library that cannot bound its own
        // latency against a dead sequencer is not usable from a UI.
        let seq = Sequencer::with_timeout(Some("http://192.0.2.1".into()), Duration::from_secs(2));
        let err = seq.account(&AccountId::from_str(ENTRY_ACCOUNT).unwrap()).unwrap_err();
        assert!(matches!(err, SdkError::Chain(_)), "{err:?}");
    }

    /// Against the live testnet. Read-only: no key material, no instructions.
    /// `RPC=https://testnet.lez.logos.co cargo test live_ -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_reads_the_deployed_entry() {
        let seq = Sequencer::new(std::env::var("RPC").ok());
        println!("tip: block {}", seq.last_block_id().unwrap());
        let entry = seq.entry(&image()).unwrap().expect("the registry's own entry is on chain");
        assert_eq!(entry.name, b"Provenance");
        assert_eq!(entry.authority, image_id_from_hex(AUTHORITY_HEX).unwrap());
        println!("entry revision {} registered_at {}", entry.revision, entry.registered_at);
        // undiscovered buckets and unclaimed third-party spaces answer None, not error
        assert!(seq.entry(&[0x11u8; 32]).unwrap().is_none());
        assert!(seq.bucket(0, 0).unwrap().is_none());
        assert!(seq
            .claim(&image(), &AccountId::from_str(AUTHORITY_BASE58).unwrap())
            .unwrap()
            .is_none());
        assert!(seq.transaction("00".repeat(32).as_str()).unwrap().is_none());
    }
}
