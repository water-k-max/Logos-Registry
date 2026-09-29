#!/usr/bin/env bash
# One-shot reproduction of the deployed `provenance` guest: stage -> docker build -> pack -> ImageID.
# Mirrors the recipe that produced the on-chain program; every input is overridable so a verifier
# on another machine can point it at its own paths.
#
#   scripts/build_guest.sh [context_dir] [output_dir]
#   SRC=... GITDB=... PACKAGER=... IMGID=... CACHE_ID=... REGCACHE=... VENDOR=... scripts/build_guest.sh
#
# REGCACHE=<dir of .crate files + index cache> switches the container to a SEALED build: the
# recipe copies it to /root/.cargo/registry and sets CARGO_NET_OFFLINE=true, so a cold build
# needs no network. Without it, a cold build must reach crates.io (measured: times out here).
#
# VENDOR=<dir> is the alternative to BOTH cargo caches: `cargo vendor` output for
# methods/guest, installed into the context as `vendor/` together with
# docker/vendor-cargo-config.toml at `.cargo/config.toml`. The dependency sources are then
# *declared content of the input set*, so no cache is a build input and the pinned upstream
# image alone suffices. In this mode the context gets no gitdb/ and no regcache/.
#
# Prints `IMAGEID <hex>` as its last line; `lezbuild verify --build` reads that line.
set -euo pipefail

SRC="${SRC:-/home/user/provenance}"
CTX="${1:-/home/user/prov-build}"
OUT="${2:-$SRC/artifacts/verify}"
GITDB="${GITDB:-/home/user/lez-programs/gitdb}"
PACKAGER="${PACKAGER:-$SRC/target/debug/risc0-packager}"
IMGID="${IMGID:-/home/user/imgid/target/release/imgid}"
CACHE_ID="${CACHE_ID:-provenance-risc0-guest-b2}"
REGCACHE="${REGCACHE:-}"
VENDOR="${VENDOR:-}"
# PULL_DEPS=1 is the null hypothesis of the whole seed argument: stage nothing, forbid nothing, and
# let cargo fetch both caches from the network inside the container. If that reproduces the deployed
# ImageID, the seeds are an convenience for flaky-network verifiers, not a build input.
PULL_DEPS="${PULL_DEPS:-}"
# SEEDED=1 means the builder image already carries both cargo caches
# (docker/builder-seeded.Dockerfile, scripts/build_seeded_builder.sh), so the context needs no
# gitdb/regcopy seed trees and neither source directory is required. BASE_TAG is the tag to build
# FROM — it must be a tag under risczero/risc0-guest-builder, which is what the alias step of
# build_seeded_builder.sh creates.
SEEDED="${SEEDED:-}"
BASE_TAG="${BASE_TAG:-}"
DF="$SRC/docker/build-guest.Dockerfile"

if [ -n "$BASE_TAG" ]; then
    test -n "$SEEDED" || { echo "ABORT: BASE_TAG set but SEEDED unset — a non-seeded image with a fresh cache must reach crates.io"; exit 2; }
fi
test -n "$SEEDED" || test -n "$VENDOR" || test -n "$PULL_DEPS" || test -d "$GITDB" || { echo "ABORT: cargo git db $GITDB missing (offline seed)"; exit 2; }
test -f "$DF"            || { echo "ABORT: $DF missing"; exit 2; }
test -x "$PACKAGER"      || { echo "ABORT: packager $PACKAGER missing (cargo build -p risc0-packager)"; exit 2; }
test -x "$IMGID"         || { echo "ABORT: imgid $IMGID missing"; exit 2; }
docker info >/dev/null   || { echo "ABORT: docker daemon not reachable"; exit 2; }

echo "==> staging $SRC -> $CTX (no target/, no .git/, no artifacts/)"
rm -rf "$CTX"
mkdir -p "$CTX"
rsync -a --exclude 'target/' --exclude '.git/' --exclude 'artifacts/' "$SRC"/ "$CTX"/
if [ -n "$PULL_DEPS" ]; then
    echo "==> PULL_DEPS: no caches, no vendor tree staged; the container must fetch its own inputs"
elif [ -n "$VENDOR" ]; then
    test -d "$VENDOR" || { echo "ABORT: vendor dir $VENDOR missing (cargo vendor --locked --manifest-path methods/guest/Cargo.toml $VENDOR)"; exit 2; }
    VCFG="$SRC/docker/vendor-cargo-config.toml"
    test -f "$VCFG" || { echo "ABORT: $VCFG missing (source-replacement config for the vendor dir)"; exit 2; }
    echo "==> VENDOR: declared sources ($(du -sh "$VENDOR" | cut -f1), $(ls -1 "$VENDOR" | wc -l) packages); no gitdb/, no regcache/ in the context"
    mkdir -p "$CTX/vendor" "$CTX/.cargo"
    cp -al "$VENDOR"/. "$CTX/vendor"/ 2>/dev/null || cp -a "$VENDOR"/. "$CTX/vendor"/
    cp "$VCFG" "$CTX/.cargo/config.toml"
elif [ -n "$SEEDED" ]; then
    echo "==> SEEDED: caches come from the image; context carries no seed trees"
else
    mkdir -p "$CTX/gitdb"
    cp -a "$GITDB"/. "$CTX/gitdb"/
fi
if [ -n "$REGCACHE" ]; then
    test -d "$REGCACHE" || { echo "ABORT: regcache $REGCACHE missing"; exit 2; }
    echo "==> SEALED: seeding crates.io cache from $REGCACHE ($(du -sh "$REGCACHE" | cut -f1))"
    mkdir -p "$CTX/regcache"
    cp -a "$REGCACHE"/. "$CTX/regcache"/
fi
cp "$DF" "$CTX/build-guest.Dockerfile"

BUILD_ARGS=(--build-arg "PROV_BUILD_CACHE_ID=$CACHE_ID")
[ -n "$BASE_TAG" ] && BUILD_ARGS+=(--build-arg "RISC0_DOCKER_CONTAINER_TAG=$BASE_TAG")
# NETWORK=none makes "with the network switched off" a container-level fact rather than a
# cargo config flag: nothing in the RUN step can reach a registry even if CARGO_NET_OFFLINE
# were unset.
NETWORK="${NETWORK:-}"
[ -n "$NETWORK" ] && BUILD_ARGS+=(--network "$NETWORK")

echo "==> docker build ($CACHE_ID${BASE_TAG:+, base $BASE_TAG}${NETWORK:+, network $NETWORK})"
mkdir -p "$OUT"
DOCKER_BUILDKIT=1 docker build \
  -f "$CTX/build-guest.Dockerfile" \
  "${BUILD_ARGS[@]}" \
  --output="$OUT" \
  "$CTX" > "$OUT/build.log" 2>&1 || {
    echo "DOCKER FAILED — tail of build.log:"; tail -40 "$OUT/build.log"; exit 1; }

echo "==> packing + ImageID"
"$PACKAGER" "$OUT/provenance.elf" "$OUT/provenance.bin"
sha256sum "$OUT/provenance.elf" "$OUT/provenance.bin" | tee -a "$OUT/build.log"
ID=$("$IMGID" "$OUT/provenance.bin" | cut -d' ' -f1)
echo "IMAGEID $ID" | tee -a "$OUT/build.log"
