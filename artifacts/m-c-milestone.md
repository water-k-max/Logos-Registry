# M-C milestone — the seed-delivery channel, measured 2026-09-29 00:07

## The question M-C answers

A "reproducible build" claim is only as good as its **input set**. Measured earlier (§0.9,
re-confirmed here): `Cargo.lock` + the pinned builder image + the repo are **not** sufficient.
`cargo build --locked` with an empty cache and no network dies on `[28] Timeout was reached`,
because the crate *sources* live in two cargo caches that are not described by the lockfile:

| seed        | content                          | size  |
|-------------|----------------------------------|-------|
| git DB      | `logos-execution-zone` + friends | 389 M / 184 files |
| registry    | crates.io `.crate` files + index | 252 M / 2370 files |

So the real input set is four things: **repo, builder image, Cargo.lock, two caches**.
M-C decides *how those two caches reach a stranger*.

## What was built

1. `docker/builder-seeded.Dockerfile` — derived image that carries the seeds read-only at
   `/opt/prov-seed/{gitdb,regcache}` and sets `CARGO_NET_OFFLINE=true`.
2. `docker/build-guest.Dockerfile` — stages `/opt/prov-seed/X/. -> /src/X/` when the context
   supplied no seed tree, then the pre-existing seed logic fills the BuildKit cache mount.
3. `scripts/build_seeded_builder.sh` — builds + aliases that image (`cp -al` hardlink staging,
   so the 640 MB is not duplicated on disk).
4. `scripts/image_only_reproduction.sh` — the stranger test: fresh cache id, `--network none`,
   context with **no** seed trees.
5. `scripts/check_image_seed.sh` — verifies the image's seed content *before* a 15-minute build.
6. `tools/lezbuild`: `Recipe.image_seeds` + `offline_seed.caches_baked_in_builder_image` in the
   manifest + test assertions, so a third input channel cannot be invisible to the manifest.

## The finding that shaped the design (and cost one failed build)

Baking into `/root/.cargo/{git,registry}` — the path cargo actually reads — is **a no-op**.
A BuildKit `--mount=type=cache` at a path *shadows* image content at that path: the step sees a
fresh, empty volume instead of the baked files. First cold attempt failed inside the existing
seed guard with `cp: cannot stat '/src/gitdb/.'`. That is why the seed has its own path
(`/opt/prov-seed`) and the recipe copies it, rather than the image "being the cargo cache".

Consequence for how we talk about it: the cache mounts are a **build accelerator**, never part
of the reproducibility claim. Only `/opt/prov-seed` is content-addressed, and only by the image
digest.

## Measured result

`SEEDED=1 BASE_TAG=r0.1.88.0-provseed-20260928 CACHE_ID=prov-imgonly-1790636691 NETWORK=none`

- context listing: no `gitdb/`, no `regcache/` (the proof the seeds came from the image)
- `#8 1.809 STAGED-FROM-IMAGE gitdb`, `#8 22.70 STAGED-FROM-IMAGE regcache`
- `SEEDED 389M /root/.cargo/git/db`, `SEEDED-REGISTRY 252M /root/.cargo/registry`,
  `==> SEALED BUILD: registry from /src/regcache, cargo network disabled`
- **166 `Compiling` lines**, `Finished release profile [optimized] in 2m 07s`
- in-container `sha256sum`: `a1a0fdc88a0e06b82d2e36abc0f3b192fdf8be2914823a8784c5b291e1f56089`
- host: ELF 506,164 B, `.bin 6bd3e46c59b50f4be28702811cf69ab0dd28db640425efc287ea0ecb8c2b887b`
- **`IMAGEID 7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6`** = the deployed ID

Image: `sha256:e3708863a09a686ca22ad76578f1f57d5e7c6f8867f28f2b2806f86460bbb659`, 2,198,177,889 B,
base `sha256:3e12f71bacd27527a61dea96fa0e53e468c99aa261d3a1019b593f6dbd943eb3` (pinned upstream
`r0.1.88.0`).

Stated rather than smoothed over: the exported ELF's mtime is identical (`2026-09-28T12:25:54.793134948Z`)
in this run and the earlier `verify/` run, so `docker --output` does not stamp the export with the
wall clock. The evidence of *this* build is the hash printed inside its own log at step+158 s,
not the file's mtime.

## What is still NOT true (the honest gap, and it is a prize-relevant one)

1. **The seeded image exists only in this Docker daemon.** The tag is not pushed anywhere, so a
   stranger today must rebuild it (chicken-and-egg: needs both caches) or receive a ~2.2 GB
   `docker save` tar. Real options: push to a registry and pin the digest, or ship the tar as a
   release asset.
2. **Nothing on chain names the builder.** The live entry's `builder_image_digest`, `source_cid`,
   `manifest_cid`, `commit`, `dep_audit_hash` are all UNSET (verified by reading the account).
   The reproducible-build claim currently lives in `artifacts/reproducible-build.json`, not in the
   registry account. Filling it in is an `update`/`attach_manifest` **write** (entry `revision`
   1 -> 2) and needs the operator's approval.
3. `CARGO_NET_OFFLINE=true` is baked in the image *and* exported by the recipe: a build that
   starts needing network fails loudly instead of phoning home — but the seed set must be
   regenerated whenever `Cargo.lock` gains a crate.

## SDK side, same session

- `sdk/src/chain.rs` now `pub use`s `AccountId`/`ProgramId` — both appear in public signatures, so
  without it an integrator could not name the type without taking their own `lee_core` dependency.
  19 passed / 2 ignored / 8.41 s.
- `sdk/examples/explorer-resolve.rs`: ImageID in -> RPC, tip block, registry namespace, derived
  entry account, decoded record out. Exit codes are a contract: `0` registered, `3` unregistered,
  `2` bad invocation, `1` chain/decode failure — all four measured live against the testnet.
- `idl_cid` prints as raw hex on purpose: it is `sha256sum provenance-idl.json`, and wrapping a
  bare digest as a CID would assert a codec the chain never recorded.

## Reproduce it yourself

```
scripts/build_seeded_builder.sh                 # once, builds the image
scripts/check_image_seed.sh                     # confirm the image carries the seeds
scripts/image_only_reproduction.sh              # cold, --network none, no context seeds
scripts/run_example.sh                          # sdk tests + the example's four exit paths
```
All four are read-only with respect to the chain. No instruction was submitted for M-C.
