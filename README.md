# Provenance — an LEZ Program Registry

A SPEL/LEZ program that gives an otherwise anonymous RISC Zero `ImageID` a public, first-claim-wins
identity record: who registered it, what repository and build recipe claim to produce it, and what
independent verification has been published against it.

This repository contains the guest program, the host tooling that builds and registers it, and the
evidence that the deployed binary can be reproduced from a pinned, offline input set.

## What is deployed

| | |
|---|---|
| Program (LEZ) | `provenance`, built with [spel-framework](https://github.com/logos-co/spel) |
| ImageID = program ID | `7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6` |
| same, base58 | `9J7yRA8WADpGRtSDaQ4TFyPz37hwD6JaSJTNteUEUU77` |
| guest ELF | `a1a0fdc88a0e06b82d2e36abc0f3b192fdf8be2914823a8784c5b291e1f56089`, 506,164 B |
| guest `.bin` | `6bd3e46c59b50f4be28702811cf69ab0dd28db640425efc287ea0ecb8c2b887b` |
| network | `https://testnet.lez.logos.co/` |
| public source | `https://github.com/water-k-max/Logos-Registry`, branch `main`, visibility `public` (re-checked anonymously on 2026-09-30) |
| registry entry for this program | account `Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ` (PDA under the registry's own namespace), `REGISTERED`. The source re-attach described under *Status* writes on-chain revision **3**; revision **2** is what the record held before that write. |
| registering authority | `8tWS2X8e59Q4FYUUExxxFzpNjbQirzkFjDjYSukVzsqe` |
| block explorer | `https://explorer.testnet.lez.logos.co/account/9J7yRA8WADpGRtSDaQ4TFyPz37hwD6JaSJTNteUEUU77` |

## Verify it from the outside — one command

You do not need my machine, my cargo caches or my network access. You need this repository, the
pinned builder image (or the seed bundle it is built from), and Docker. The repository is public at
`https://github.com/water-k-max/Logos-Registry`; cloning it is input (1) of the build.

```bash
BUILD=1 bash scripts/verify_from_the_outside.sh
```

That rebuilds the guest in a container with **no network** and a **brand-new** BuildKit cache, then
compares three things that must agree:

1. the ELF and ImageID this run just produced, against `artifacts/reproducible-build.json`;
2. that manifest, against the Tier-3 record stored on the live testnet account;
3. the manifest file's own `sha256` against the on-chain `manifest_cid`.

If you are starting from the seed bundle rather than an already-present builder image:

```bash
BUILD=1 BUNDLE=/path/to/prov-seeds.tar.gz BASE_TAG=r0.1.88.0-provseed-bundle1 \
  bash scripts/verify_from_the_outside.sh
```

`BUNDLE=` verifies the archive against `artifacts/seed-bundle.sha256`, unpacks the two cargo caches,
and rebuilds the builder image from them before doing the above.

Exit codes are part of the contract, and the contract is pinned by
`bash scripts/check_outside_exits.sh` (6 cases):

| code | meaning |
|---|---|
| 0 | build, manifest, source tree and chain all agree |
| 1 | the build this machine just did does not match the manifest |
| 2 | the chain and the manifest disagree — the on-chain claim is stale, do not publish it |
| 3 | environment problem (missing bundle, unparseable manifest, absent field) |
| 4 | this checkout does not hash to the manifest's `source_cid` — the source claim is unverifiable |

Code 4 is the one that is easy to omit: a README edit or a new script moves the whole-tree hash
without moving the guest, so a verifier that compared only *build ↔ manifest ↔ chain* would print
VERIFIED about a source record nobody can reconstruct. It runs before the expensive build, and
`TREE_CHECK=` skips it when you want the other legs anyway.

## What the build actually depends on

Four inputs, all pinned, all measured — not "repo + Dockerfile":

1. this repository,
2. the builder image `risczero/risc0-guest-builder@sha256:3e12f71bacd27527a61dea96fa0e53e468c99aa261d3a1019b593f6dbd943eb3`
   (`r0.1.88.0`, i.e. `rustc 1.88.0-dev (de85b1d3d)`),
3. `methods/guest/Cargo.lock` (821 external crates + 2 local crates),
4. two cargo caches: the git database (389 MB, 184 files) and the crates.io registry cache
   (252 MB, 2,370 files).

Input (4) is not a convenience, and neither is the path inside the image. Three measurements settle
the input set:

- **The vendored input set is a *different program*.** `cargo vendor` (431 packages, 369 MB) builds
  cleanly on the same pinned base digest with `--network none`, but produces ELF
  `c06db72e…` / 504,788 B and ImageID `c9753252bba0ca577e0e580c6f4cdc965e7184e4a4a21600d5e877bc5958a52d`
  — not `7b4040d3…`. The `.text` section is identical; the difference is in `.rodata`
  (0x6c44 vs 0x66d4). The guest ELF embeds each dependency's *materialization path* (`file!()` in
  panic locations), so **the absolute cargo home is itself a build input**. Any recipe that changes
  where crates land on disk changes the program ID. See `scripts/vendor_reproduction.sh`.
- **That inference is now a controlled result.** With every other input fixed (same base image, same
  seeds, same rustflags, `--network none`, empty build caches), changing only `CARGO_HOME` moves the
  ImageID: `/root/.cargo` reproduces the deployed `7b4040d3…`, `/root/.carg1` — the *same length*,
  one character different — gives `cbc6a8cf…` with a **byte-identical `.text`** and exactly 25
  differing bytes of `.rodata`, and `/opt/alt-cargo` gives `0e94b5bf…`. One character in the build
  environment is a different program, which is why the pinned image is an input and not a
  convenience. See `scripts/cargo_home_experiment.sh` and `scripts/cargo_home_sections.sh`.
- **A cold build that fetches its own dependencies does not work here.** With network enabled and no
  caches it fails at ~243 s with `revision 47eba25… not found` and an SSL error (exit 101), even
  though `Updating crates.io index` succeeds. That is scoped to this egress path, not a claim about
  your network. See `scripts/pull_deps_reproduction.sh`.

So the dependency set is delivered as content, not as a network location: a deterministic
**seed bundle** (`prov-seeds.tar.gz`, 598,990,268 B, sha256
`1a8ef1f41d4bf07edcf6f6a8876f1caf71acac9529038f595553fcf6536ed2df`) plus the public base image. The
seeded builder image (`provenance-builder-seeded`, built by `scripts/build_seeded_builder.sh`) is an
**accelerator**, not part of the claim: rebuilding it from the bundle yields layer checksums that
match, while its docker image ID differs (`e3708863a09a` vs `efbf337c7809`) because image IDs embed
build timestamps. The program is the claim.

One BuildKit rule shaped this: `--mount=type=cache` at a path **shadows** image content at that
path. Baking seeds into `/root/.cargo/{git,registry}` is therefore a no-op; the image carries them
read-only at `/opt/prov-seed/{gitdb,regcache}` and the recipe stages them into `/src` only when the
build context supplied none.

## Tools in this repository

| path | what it does |
|---|---|
| `methods/guest/src/bin/provenance.rs` | the on-chain program (`#[lez_program]`) |
| `provenance_core/` | shared types used by the guest |
| `sdk/` | host-side read path: sequencer RPC, PDA derivation, entry decoding, `ImageID` → registry lookup (`cargo run --example explorer-resolve -- <image-id>`, from `sdk/`) |
| `tools/lezbuild/` | builds the reproducibility manifest, and `verify`s a tree against it (exit code says which claim broke) |
| `tools/lezreg/` | builds and submits registry instructions, including `attach-manifest` (the Tier-3 record now on chain) |
| `tools/risc0-packager/` | ELF → `.bin` (segment wrapping), so ImageID is computed from what actually deploys |
| `docker/` | the sealed guest recipe and the seeded builder image |
| `scripts/` | the measurements above, plus the one-command verifier and its exit-code harness |

The scaffolding targets (`make build`, `make idl`, `make deploy`, `make cli ARGS="--help"`, see
`Makefile`) are the standard spel workflow and are what this program was developed against. The path
behind the reproducibility claim, however, is `scripts/build_guest.sh`, not `make build` — `make
build` uses whatever cache and network state your machine happens to have, which is exactly what
this document is trying to pin down.

## Status and honest limits

- **Reproducible: measured, not asserted.** The full chain (seed bundle → builder image rebuilt from
  it → cold sealed build with `--network none` and a brand-new cache id → manifest → live testnet
  record) has been run end to end three times on 2026-09-29, at tip blocks 30232 and 30542 (twice):
  166 crates compiled in 1m16s–2m07s, ELF `a1a0fdc8…` / 506,164 B, `ImageID` `7b4040d3…` — identical
  to what is deployed. The third run happened *after* 28 GB of build caches were deleted, which is
  the point: the claim does not depend on anything cached on this machine. Logs:
  `artifacts/outside-full.log`, `artifacts/outside-postreclaim.log`.
- **Published is not the same claim as verifiable, so they are measured separately.** This tree is on
  GitHub at `https://github.com/water-k-max/Logos-Registry` (`main`, public). The history got there
  without rewriting anything a reviewer could already see: the frozen commit was merged with the
  repository's initial commit (`.gitattributes` resolved to the `* -text` version, so the merged tree
  oid equals the frozen tree oid) and the push was a fast-forward; the pre-publication placeholder is
  still reachable on the branch `placeholder-initial`. The check worth running goes one step past the
  git object store: fetch what GitHub actually *serves*, then hash the pruned file set and compare it
  to the manifest. Measured that way on 2026-09-30 — the tarball of `main` unpacked to 159 files /
  6,543,880 B with `* -text` intact, 87 of them outside the pruned set and therefore covered by
  `source_cid`, and `lezbuild verify` against the extracted tree reported a `source_tree_actual` equal
  to the manifest's `source_cid`, `GUEST GRAPH MATCHES`, exit 0. Those file and byte counts describe one
  tree at one moment; the invariant is the equality, not the numbers.
- **No document inside this tree can quote this tree's hash, so none does.** `source_cid` is a sha256
  over sorted `(path, content-sha256)` pairs for every file outside the pruned set (`target`, `.git`,
  `artifacts`, `gitdb`, `regcache`, `.cargo`, `node_modules` — see `PRUNE` in
  `tools/lezbuild/src/hash.rs`), and that drives three consequences this project lives by: a README
  edit moves `source_cid` without moving the guest, which is why the verifier runs the tree leg
  *before* the expensive build and exits 4 rather than printing VERIFIED about a source nobody can
  reconstruct; `artifacts/` being pruned is what lets the manifest be regenerated after the
  documentation is final without moving `source_cid` (its own sha256, the on-chain `manifest_cid`, is a
  separate value that stays independently checkable); and no file can carry either its tree's hash or
  the id of the commit that carries it — writing either would change both. So the `source_cid` and the
  pinned `commit` live in `artifacts/reproducible-build.json` and in the Tier-3 record, never here, and
  `bash scripts/verify_from_the_outside.sh` with the tree leg enabled is what proves they still agree
  with your clone. The manifest also lists every pruned-free file with its own sha256, so when the tree
  leg fails the output names *which* file moved instead of just saying "stale".
- **The on-chain `repo_url` and `commit` are being written, and were zero until now on purpose.**
  Before the freeze they were empty/zero because a commit hash with no resolvable referent is a worse
  claim than a blank field; `commit` in particular could not be set while the repository was not
  public. The value names the last commit that touched a *hashed* file. Later commits are allowed to
  exist and to change only pruned paths (regenerated evidence, rebuilt manifests) without invalidating
  it — which is the property that lets this document be final before the numbers in it are refreshed.
  `builder_digest` and `dep_audit_hash` are unchanged by the freeze — the guest did not move, and the
  whole point of exit 4 is to distinguish "the documentation changed" from "the program changed".
- **Byte-exact checkouts are part of the claim, so `.gitattributes` pins them.** `source_cid` is a
  sha256 over file *bytes*, and `* -text` disables end-of-line conversion for every path. Without it,
  a clone with `core.autocrlf=true` (the default on many Windows setups) rewrites LF to CRLF, and the
  reviewer's tree no longer hashes to the published value even though the program is bit-identical.
  If you are on Windows, verify with `git config core.autocrlf input` or just trust this file: it is
  the reason it exists.
- **Tests: 47 passing, re-measured after the cache deletion, and one that still fails.** Regenerate
  with `bash scripts/m_b_evidence.sh` (it recompiles the workspace from source, so the first run on a
  fresh clone is slow); its output is `artifacts/m-b-evidence.txt`, and section 11 names the commit
  the numbers describe. Current sweep (2026-09-29, commit `36a319d`): `provenance_core` 5,
  `lezreg` 14 (2 ABI + 12 CLI-contract), `lezbuild` 9, `sdk` 19 → **47 hermetic passing**, plus the
  live chain read passing against the testnet, `RISC0_SKIP_BUILD=1 cargo check --offline --workspace
  --all-targets` exit 0, and the verifier's six-case exit-code contract holding
  (`verify` 0, `verify --strict` 2). **The storage REST smoke test fails** (exit 101 after 30 s):
  there is no local Logos Storage gateway on `:8080`. It is left failing rather than skipped, because
  a test that silently skips proves nothing.
- **Delivery of the seed bundle is still the open half of reproducibility.** The bundle
  (`prov-seeds.tar.gz`, 598,990,268 B, sha256
  `1a8ef1f41d4bf07edcf6f6a8876f1caf71acac9529038f595553fcf6536ed2df`) is the *content* of inputs (3)
  and (4), and it currently exists on this machine and nowhere else. The intended delivery is a GitHub
  release asset beside this repository: if you are reading this in the published repository and that
  release has no such asset, then reproducing from cold still requires the archive to be handed over
  out of band, and that gap is real rather than papered over. What has been measured is the local path
  (`BUNDLE=` verifies the archive against `artifacts/seed-bundle.sha256` before using it, then rebuilds
  the builder image from it). The 2.2 GB seeded builder image is an accelerator anyone can rebuild from
  the bundle, not something a verifier is obliged to trust.
- The builder image digest on chain is the **upstream base** digest (`3e12f71b…`), because that is
  the pin the claim actually depends on; the seeded wrapper image is derived content.
- `scripts/*` are run with `bash <script>`; several carry non-obvious environment caveats recorded
  in their own headers.

