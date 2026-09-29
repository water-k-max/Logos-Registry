#![cfg_attr(not(test), no_main)]

use spel_framework::prelude::*;

#[cfg(not(test))]
risc0_zkvm::guest::entry!(main);

#[lez_program]
mod provenance {
    #![allow(clippy::too_many_arguments)]

    use clock_core::{ClockAccountData, CLOCK_01_PROGRAM_ACCOUNT_ID};
    use nssa_core::account::{Account, AccountWithMetadata, Data};
    use provenance_core::{Entry, IndexBucket, MetaError, ThirdPartyClaim, Verification};

    fn spel_err(e: MetaError) -> SpelError {
        SpelError::custom(e.code() as u32, e.msg().to_string())
    }

    fn decode<T: borsh::BorshDeserialize>(acct: &AccountWithMetadata) -> Result<T, SpelError> {
        T::try_from_slice(&acct.account.data)
            .map_err(|_| SpelError::custom(900, "account decode failed"))
    }

    fn encode<T: borsh::BorshSerialize>(
        acct: &mut AccountWithMetadata,
        value: &T,
    ) -> Result<(), SpelError> {
        let bytes =
            borsh::to_vec(value).map_err(|_| SpelError::custom(901, "account encode failed"))?;
        acct.account.data =
            Data::try_from(bytes).map_err(|_| SpelError::custom(902, "account data too big"))?;
        Ok(())
    }

    /// Millisecond wall clock from the system CLOCK_01 account (the pinned
    /// `ProgramContext` exposes no clock — same pattern as official guests).
    fn now_ms(clock: &AccountWithMetadata) -> Result<u64, SpelError> {
        if *clock.account_id.value() != *CLOCK_01_PROGRAM_ACCOUNT_ID.value() {
            return Err(SpelError::custom(903, "clock account must be CLOCK_01"));
        }
        if clock.account == Account::default() {
            return Err(SpelError::custom(904, "clock account is uninitialized"));
        }
        ClockAccountData::try_from_slice(&clock.account.data)
            .map(|c| c.timestamp)
            .map_err(|_| SpelError::custom(905, "clock account decode failed"))
    }

    /// Create the canonical first-claim entry for a LEZ program (ImageID).
    ///
    /// The `["entry", program_id]` PDA can only be initialized once, so the
    /// first signed register wins and any later deployer-class claim for the
    /// same ImageID is rejected by construction. Entries are labeled
    /// "first-claim attested", never "proven deployer" — LEZ deployments carry
    /// no signer.
    #[instruction]
    pub fn register(
        #[account(init, pda = [literal("entry"), arg("image_id")])]
        mut entry: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        clock: AccountWithMetadata,
        image_id: [u8; 32],
        name: Vec<u8>,
        version: Vec<u8>,
        author_name: Vec<u8>,
        description: Vec<u8>,
        tags: Vec<Vec<u8>>,
        idl_cid: [u8; 32],
    ) -> SpelResult {
        let now = now_ms(&clock)?;
        let record = Entry::new(
            image_id,
            *authority.account_id.value(),
            name,
            version,
            author_name,
            description,
            tags,
            idl_cid,
            now,
        )
        .map_err(spel_err)?;
        encode(&mut entry, &record)?;
        Ok(SpelOutput::execute(vec![entry, authority, clock], vec![]))
    }

    /// Author-only metadata update (version/idl/description/tags).
    #[instruction]
    pub fn update(
        #[account(mut, pda = [literal("entry"), arg("image_id")])]
        mut entry: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        clock: AccountWithMetadata,
        image_id: [u8; 32],
        name: Vec<u8>,
        version: Vec<u8>,
        author_name: Vec<u8>,
        description: Vec<u8>,
        tags: Vec<Vec<u8>>,
        idl_cid: [u8; 32],
    ) -> SpelResult {
        let now = now_ms(&clock)?;
        let mut record: Entry = decode(&entry)?;
        record
            .update(
                *authority.account_id.value(),
                name,
                version,
                author_name,
                description,
                tags,
                idl_cid,
                now,
            )
            .map_err(spel_err)?;
        encode(&mut entry, &record)?;
        Ok(SpelOutput::execute(vec![entry, authority, clock], vec![]))
    }

    /// Record the Tier-3 reproducible-build claims (repo, commit, source and
    /// build-manifest CIDs, pinned builder digest, dep-audit report hash) on
    /// an entry the caller authored.
    #[instruction]
    pub fn attach_manifest(
        #[account(mut, pda = [literal("entry"), arg("image_id")])]
        mut entry: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        clock: AccountWithMetadata,
        image_id: [u8; 32],
        repo_url: Vec<u8>,
        commit: [u8; 32],
        source_cid: [u8; 32],
        manifest_cid: [u8; 32],
        builder_image_digest: [u8; 32],
        dep_audit_hash: [u8; 32],
    ) -> SpelResult {
        let now = now_ms(&clock)?;
        let mut record: Entry = decode(&entry)?;
        record
            .attach_manifest(
                *authority.account_id.value(),
                repo_url,
                commit,
                source_cid,
                manifest_cid,
                builder_image_digest,
                dep_audit_hash,
                now,
            )
            .map_err(spel_err)?;
        encode(&mut entry, &record)?;
        Ok(SpelOutput::execute(vec![entry, authority, clock], vec![]))
    }

    /// A third party's own claim about a program ID. Lives in a separate PDA
    /// space from `Entry`, so deployer-class vs third-party is structural.
    #[instruction]
    pub fn attest(
        #[account(init, pda = [literal("claim3p"), arg("image_id"), account("authority")])]
        mut claim: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        clock: AccountWithMetadata,
        image_id: [u8; 32],
        label: Vec<u8>,
    ) -> SpelResult {
        let now = now_ms(&clock)?;
        let record = ThirdPartyClaim::new(
            image_id,
            *authority.account_id.value(),
            label,
            now,
        )
        .map_err(spel_err)?;
        encode(&mut claim, &record)?;
        Ok(SpelOutput::execute(vec![claim, authority, clock], vec![]))
    }

    /// Publish a verification record: an Ed25519 signature (under
    /// `verifier_key`) over the 32-byte digest committing to
    /// (program_id, commit, source_cid, manifest_cid, image_digest). The
    /// on-chain record is bound to the signing `verifier` account by the PDA;
    /// readers re-check `sig` client-side with their own trust set.
    #[instruction]
    pub fn publish_verification(
        #[account(init, pda = [literal("vfy"), arg("image_id"), account("verifier")])]
        mut vfy: AccountWithMetadata,
        #[account(signer)] verifier: AccountWithMetadata,
        clock: AccountWithMetadata,
        image_id: [u8; 32],
        verifier_key: [u8; 32],
        sig: Vec<u8>,
        v: [u8; 32],
    ) -> SpelResult {
        let now = now_ms(&clock)?;
        let sig: [u8; 64] =
            sig.try_into().map_err(|_| SpelError::custom(906, "sig must be exactly 64 bytes"))?;
        let record = Verification::new(
            image_id,
            *verifier.account_id.value(),
            verifier_key,
            sig,
            v,
            now,
        );
        encode(&mut vfy, &record)?;
        Ok(SpelOutput::execute(vec![vfy, verifier, clock], vec![]))
    }

    /// Create an empty discovery bucket shard.
    #[instruction]
    pub fn index_init(
        #[account(init, pda = [literal("index"), arg("bucket"), arg("shard")])]
        mut index: AccountWithMetadata,
        bucket: u32,
        shard: u32,
    ) -> SpelResult {
        let record = IndexBucket::empty(bucket, shard);
        encode(&mut index, &record)?;
        Ok(SpelOutput::execute(vec![index], vec![]))
    }

    /// Append a program ID to its bucket (bucket = `program_id[0]`);
    /// idempotent, errors `BucketFull` when the caller must open the next
    /// shard.
    #[instruction]
    pub fn index_append(
        #[account(mut, pda = [literal("index"), arg("bucket"), arg("shard")])]
        mut index: AccountWithMetadata,
        image_id: [u8; 32],
        bucket: u32,
        shard: u32,
    ) -> SpelResult {
        let mut record: IndexBucket = decode(&index)?;
        record.push(image_id).map_err(spel_err)?;
        encode(&mut index, &record)?;
        Ok(SpelOutput::execute(vec![index], vec![]))
    }
}
