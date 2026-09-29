//! CLI contract for `lezreg`: a wrong invocation is a readable error and exit 2, and the
//! derived entry account is the **registry's** PDA namespace, not the subject's.
//!
//! The namespace case is the regression test for the defect recorded in
//! IMPLEMENTATION_PLAN §0.10: an entry for someone else's program used to be derived from
//! that program's ImageID, which put it in the wrong space and dispatched `register` to the
//! wrong program. The registry's self-entry is the one subject where both derivations agree,
//! so `Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ` passing tells you nothing — `ee..ee`
//! is the case that has to be pinned.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lezreg");
const REGISTRY: &str = "7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6";
const SELF_ENTRY: &str = "Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ";
/// compute_pda(registry, ["entry", ee..ee]) — agrees with the SDK's `entry_pda`.
const THIRD_PARTY_ENTRY: &str = "AK8MjziZ38Hi8urRrJ55r1VspwUSyLaiGwcNXyvEcBM7";
const AUTHORITY: &str = "8tWS2X8e59Q4FYUUExxxFzpNjbQirzkFjDjYSukVzsqe";
/// The live clock account every instruction passes as its third account.
const CLOCK: &str = "4BdcjoXkq786TMWcBGGHqcxeLYMZmn17rL4eM9ZyRWNU";

fn run(args: &[&str]) -> (String, String, i32) {
    let out = Command::new(BIN).args(args).output().expect("bin runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().expect("not killed by a signal"),
    )
}

/// Fails the test the way an operator would notice: a panic instead of an error message.
fn assert_clean_error(stdout: &str, stderr: &str, code: i32, expected: &str) {
    assert_eq!(code, 2, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stderr.contains(&format!("error: {expected}")),
        "expected 'error: {expected}', got:\n{stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "user errors must not surface as thread panics:\n{stderr}"
    );
}

/// The `accounts` line, as printed. Matching on the whole line keeps these assertions
/// independent of how the label column happens to be padded today.
fn accounts_line(stdout: &str) -> String {
    stdout
        .lines()
        .find(|l| l.starts_with("accounts"))
        .unwrap_or_default()
        .to_string()
}

/// The account list on that line, in submission order (the entry/claim PDA is first).
fn accounts(stdout: &str) -> Vec<String> {
    let line = accounts_line(stdout);
    let list = match line.split_once("accounts") {
        Some((_, rest)) => rest,
        None => line.as_str(),
    };
    list.split(',')
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect()
}

#[test]
fn a_missing_required_flag_names_the_flag_and_exits_2() {
    let (so, se, code) = run(&[
        "register",
        "--image-id",
        REGISTRY,
        "--authority",
        AUTHORITY,
        "--name",
        "Demo",
        "--version",
        "0.1.0",
        "--description",
        "demo",
    ]);
    assert_clean_error(&so, &se, code, "missing required flag --author-name");
}

#[test]
fn bad_hex_names_the_argument_instead_of_asserting_on_length() {
    let (so, se, code) = run(&["register", "--image-id", "nothex", "--authority", AUTHORITY]);
    assert_clean_error(&so, &se, code, "--image-id");

    let (so, se, code) = run(&[
        "register",
        "--image-id",
        "0102",
        "--authority",
        AUTHORITY,
    ]);
    assert_clean_error(&so, &se, code, "--image-id: expected 32 bytes (64 hex chars), got 2 bytes");
}

#[test]
fn a_missing_value_is_not_silently_taken_as_the_next_flag() {
    // Before this guard the CLI read "--name" as the authority and died on "bad account id".
    let (so, se, code) = run(&["register", "--image-id", REGISTRY, "--authority", "--name", "X"]);
    assert_clean_error(&so, &se, code, "--authority needs a value, got the next flag '--name'");
}

#[test]
fn an_unknown_command_is_reported() {
    let (so, se, code) = run(&["frobnicate", "--authority", AUTHORITY]);
    assert_clean_error(&so, &se, code, "unknown command 'frobnicate'");
}

#[test]
fn no_arguments_prints_usage_with_exit_2() {
    let (so, se, code) = run(&[]);
    assert_eq!(code, 2, "stdout:\n{so}\nstderr:\n{se}");
    assert!(se.contains("usage: lezreg <register|attest"), "{se}");
    assert!(se.contains("register needs"), "{se}");
}

#[test]
fn a_third_party_subject_is_registered_in_the_registry_namespace() {
    let subject = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let (so, se, code) = run(&[
        "register",
        "--image-id",
        subject,
        "--authority",
        AUTHORITY,
        "--name",
        "Demo",
        "--version",
        "0.1.0",
        "--author-name",
        "Demo",
        "--description",
        "demo",
        "--dry-run",
    ]);
    assert_eq!(code, 0, "stderr:\n{se}");
    assert_eq!(
        accounts(&so),
        vec![THIRD_PARTY_ENTRY, AUTHORITY, CLOCK],
        "a third-party subject must land in the registry namespace, not the self entry"
    );
    assert!(so.contains("registry (callee)     7b4040d3"), "{so}");
    assert!(so.contains("subject  (ImageID)    eeee"), "{so}");
}

#[test]
fn the_self_subject_still_derives_the_live_entry_account() {
    let (so, se, code) = run(&[
        "register",
        "--image-id",
        REGISTRY,
        "--authority",
        AUTHORITY,
        "--name",
        "Provenance",
        "--version",
        "0.1.0",
        "--author-name",
        "Strategic Edge",
        "--description",
        "LEZ program registry",
        "--dry-run",
    ]);
    assert_eq!(code, 0, "stderr:\n{se}");
    assert_eq!(
        accounts(&so),
        vec![SELF_ENTRY, AUTHORITY, CLOCK],
        "the live entry account must be unchanged, in the live account order"
    );
    assert!(so.contains("both derivations agree"), "{so}");
}

#[test]
fn overriding_the_registry_namespace_moves_the_derived_account() {
    // The flag exists so a different deployment can be targeted; it must actually change
    // the derivation, otherwise "callee" is only decoration in the printout.
    let other = "0000000000000000000000000000000000000000000000000000000000000001";
    let (so, _se, code) = run(&[
        "register",
        "--image-id",
        other,
        "--registry-image-id",
        other,
        "--authority",
        AUTHORITY,
        "--name",
        "Demo",
        "--version",
        "0.1.0",
        "--author-name",
        "Demo",
        "--description",
        "demo",
        "--dry-run",
    ]);
    assert_eq!(code, 0);
    assert!(!so.contains(SELF_ENTRY) && !so.contains(THIRD_PARTY_ENTRY), "{so}");
    let (so_default, _, _) = run(&[
        "register",
        "--image-id",
        other,
        "--authority",
        AUTHORITY,
        "--name",
        "Demo",
        "--version",
        "0.1.0",
        "--author-name",
        "Demo",
        "--description",
        "demo",
        "--dry-run",
    ]);
    assert_ne!(so, so_default, "--registry-image-id must select the namespace");
}

// --- attach-manifest -------------------------------------------------------
//
// The Tier-3 digests are stored as bare `[u8; 32]`s, so an all-zero value is indistinguishable
// on chain from an entry that never had a manifest attached. The CLI therefore requires each one
// to be *stated* — either as a real digest or as the literal token `unset`, which emits a warning
// on the way to the same zeros. These tests pin that: the requirement, the warning, and the fact
// that `unset` really is the all-zero digest (so the chain reads what the warning says).

const SOURCE_CID: &str = "4d37847bbbf393317287003f361b884144ab3b3decc6a062672169e9fd3017f2";
const MANIFEST_CID: &str = "cb8a61122a9318e52da35157b5cdcf2c2f1e38e764b9c9bf6a7b94ccfd68eb9d";
const BUILDER_DIGEST: &str = "3e12f71bacd27527a61dea96fa0e53e468c99aa261d3a1019b593f6dbd943eb3";
const DEP_AUDIT: &str = "a809d64904f1d63eea68665ae3471cc0ff7ec29259243703464900ad26ded3fb";
const COMMIT: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const ZEROS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// A complete, valid `attach-manifest` for `subject` — everything but `--repo-url`, which is
/// genuinely optional (a manifest with no published referent must not claim a URL).
fn manifest(subject: &str) -> Vec<&str> {
    vec![
        "attach-manifest",
        "--image-id",
        subject,
        "--authority",
        AUTHORITY,
        "--commit",
        COMMIT,
        "--source-cid",
        SOURCE_CID,
        "--manifest-cid",
        MANIFEST_CID,
        "--builder-image-digest",
        BUILDER_DIGEST,
        "--dep-audit-hash",
        DEP_AUDIT,
        "--dry-run",
    ]
}

/// Drop a flag *and* its value, so the test exercises "not given" rather than "empty given".
fn without<'a>(flags: &[&'a str], flag: &str) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(flags.len());
    let mut skip_value = false;
    for a in flags {
        if skip_value {
            skip_value = false;
            continue;
        }
        if *a == flag {
            skip_value = true;
            continue;
        }
        out.push(*a);
    }
    out
}

fn words(stdout: &str) -> String {
    stdout
        .lines()
        .find(|l| l.starts_with("instruction_data"))
        .unwrap_or_default()
        .to_string()
}

#[test]
fn attach_manifest_requires_every_digest_and_names_the_missing_one() {
    for flag in [
        "--commit",
        "--source-cid",
        "--manifest-cid",
        "--builder-image-digest",
        "--dep-audit-hash",
    ] {
        let (so, se, code) = run(&without(&manifest(REGISTRY), flag));
        assert_clean_error(&so, &se, code, &format!("{flag} is required"));
    }
}

#[test]
fn attach_manifest_unset_stores_zeros_and_says_so() {
    let unset = {
        let mut m = without(&manifest(REGISTRY), "--commit");
        m.extend(["--commit", "unset"]);
        m
    };
    let (so_unset, se_unset, code) = run(&unset);
    assert_eq!(code, 0, "stderr:\n{se_unset}");
    assert!(
        se_unset.contains("commit stored as zeros"),
        "`unset` must be announced, not accepted silently:\n{se_unset}"
    );

    let (so_zeros, _se, code) = run(&{
        let mut m = without(&manifest(REGISTRY), "--commit");
        m.extend(["--commit", ZEROS]);
        m
    });
    assert_eq!(code, 0);
    assert_eq!(
        words(&so_unset),
        words(&so_zeros),
        "'unset' must be exactly the all-zero digest the warning describes"
    );
    assert_ne!(words(&so_unset), words(&run(&manifest(REGISTRY)).0));
}

#[test]
fn attach_manifest_writes_the_registry_namespace_entry() {
    let (so, se, code) = run(&manifest("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"));
    assert_eq!(code, 0, "stderr:\n{se}");
    assert_eq!(
        accounts(&so),
        vec![THIRD_PARTY_ENTRY, AUTHORITY, CLOCK],
        "attach-manifest must land on the same entry register/attest use, in that account order"
    );
}

#[test]
fn attach_manifest_digests_are_positional() {
    // A field that is read from the wrong flag serializes fine and lands on chain swapped.
    // Swapping two inputs must therefore change the bytes, and change them identically to the
    // invocation that passes them the other way round.
    let swapped = {
        let mut m = without(&manifest(REGISTRY), "--source-cid");
        m = without(&m, "--dep-audit-hash");
        m.extend([
            "--source-cid",
            DEP_AUDIT,
            "--dep-audit-hash",
            SOURCE_CID,
        ]);
        m
    };
    let (a, _, code_a) = run(&manifest(REGISTRY));
    let (b, _, code_b) = run(&swapped);
    assert_eq!((code_a, code_b), (0, 0));
    assert_ne!(words(&a), words(&b), "source_cid and dep_audit_hash are not the same slot");

    // ...and no digest may be silently dropped: each one has to move the serialization.
    for flag in [
        "--commit",
        "--source-cid",
        "--manifest-cid",
        "--builder-image-digest",
        "--dep-audit-hash",
    ] {
        let altered = {
            let mut m = without(&manifest(REGISTRY), flag);
            m.extend([flag, "1111111111111111111111111111111111111111111111111111111111111111"]);
            m
        };
        let (so_alt, _, code_alt) = run(&altered);
        assert_eq!(code_alt, 0);
        assert_ne!(words(&a), words(&so_alt), "{flag} does not reach the instruction bytes");
    }
}
