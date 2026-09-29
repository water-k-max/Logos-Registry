# Reproduce the official cargo-risczero guest build (image + rustflags taken
# from the Dockerfile cargo-risczero 3.0.6 generates at build time), with one
# local change: the cargo git DB is seeded from ./gitdb so no GitHub fetch
# happens inside the container (WSL->GitHub is unreliable here).
#
# Exports the guest ELF; kernel packaging (risc0-packager) runs on the host,
# because resolving the packager's own crates.io deps inside the container adds
# a network dependency that WSL intermittently drops.
ARG RISC0_DOCKER_CONTAINER_TAG=r0.1.88.0
FROM risczero/risc0-guest-builder:${RISC0_DOCKER_CONTAINER_TAG} AS build
ARG PROV_BUILD_CACHE_ID=provenance-risc0-guest

WORKDIR /src
COPY . .

ENV CARGO_TARGET_DIR=/src/target/prov-guest
ENV RISC0_FEATURE_bigint2=""
ENV CC_riscv32im_risc0_zkvm_elf=/root/.risc0/cpp/bin/riscv32-unknown-elf-gcc
ENV CFLAGS_riscv32im_risc0_zkvm_elf="-march=rv32im -nostdlib"

RUN --mount=type=cache,id=${PROV_BUILD_CACHE_ID}-cargo-git,sharing=locked,target=/root/.cargo/git \
    --mount=type=cache,id=${PROV_BUILD_CACHE_ID}-cargo-registry,sharing=locked,target=/root/.cargo/registry \
    --mount=type=cache,id=${PROV_BUILD_CACHE_ID}-target,sharing=locked,target=/src/target/prov-guest <<'EOF'
set -eu

# Two ways the offline seeds can arrive: in the build CONTEXT (/src/gitdb, /src/regcache — what
# scripts/build_guest.sh stages) or in the BUILDER IMAGE (/opt/prov-seed/... — what
# docker/builder-seeded.Dockerfile bakes, so one image digest pins toolchain + dependency set).
# The image route has to land in /src first, and that is not cosmetic: a BuildKit cache mount
# SHADOWS whatever the image had at that path. Measured on 2026-09-28 — baking the seeds straight
# into /root/.cargo/{git,registry} produced `cp: cannot stat '/src/gitdb/.': No such file or
# directory`, because the guard below saw an empty fresh cache sitting on top of the baked tree.
# So the image seed is staged to /src and takes the identical code path as a context seed.
if [ ! -d /src/gitdb ] && [ -d /opt/prov-seed/gitdb ]; then
    mkdir -p /src/gitdb
    cp -a /opt/prov-seed/gitdb/. /src/gitdb/
    echo "STAGED-FROM-IMAGE gitdb"
fi
if [ ! -d /src/regcache ] && [ -d /opt/prov-seed/regcache ]; then
    mkdir -p /src/regcache
    cp -a /opt/prov-seed/regcache/. /src/regcache/
    echo "STAGED-FROM-IMAGE regcache"
fi

mkdir -p /root/.cargo/git/db
if [ ! -d /root/.cargo/git/db/logos-execution-zone-6bae42d7c9cadfe7 ]; then
    if [ -d /src/gitdb ]; then
        cp -a /src/gitdb/. /root/.cargo/git/db/
        du -sh /root/.cargo/git/db | sed 's/^/SEEDED /'
    else
        # Legal only in a vendored build (see VENDORED MODE below): the git dependency is
        # resolved from `vendor/` through source replacement, so an empty git cache is fine.
        echo "NO-GIT-SEED: relying on .cargo/config.toml source replacement"
    fi
fi

# SEALED MODE (opt-in: scripts/build_guest.sh with REGCACHE=<dir> puts a crates.io cache at
# /src/regcache). Measured reason it exists: with fresh BuildKit cache mounts and only the git
# DB seeded, `cargo build --locked` has to reach crates.io and dies here on
# `[28] Timeout was reached` — Cargo.lock + git DB are NOT a complete input set. The registry
# cache is a second build input, so ship it and forbid the network instead of hanging on it.
if [ -d /src/regcache ]; then
    mkdir -p /root/.cargo/registry
    if [ -z "$(ls -A /root/.cargo/registry 2>/dev/null || true)" ]; then
        cp -a /src/regcache/. /root/.cargo/registry/
        du -sh /root/.cargo/registry | sed 's/^/SEEDED-REGISTRY /'
    fi
    export CARGO_NET_OFFLINE=true
    echo "==> SEALED BUILD: registry from /src/regcache, cargo network disabled"
fi

# VENDORED MODE (opt-in: scripts/build_guest.sh with VENDOR=<dir> puts `cargo vendor` output at
# /src/vendor plus the source-replacement config at /src/.cargo/config.toml).
#
# MEASURED, 2026-09-29: this mode is REJECTED as a reproduction route. It builds cleanly
# (166 crates, 1m35s, network disabled) but it builds a DIFFERENT PROGRAM:
#   ELF     c06db72ed724a4fb5d17556d66aa307e7fb003c25046120f12be8cce838417db / 504,788 B
#   .bin    86df2c1dfb25e53949970f52ee1c1ef36a23732685d10ba0a72bbbd725df24a2
#   ImageID c9753252bba0ca577e0e580c6f4cdc965e7184e4a4a21600d5e877bc5958a52d
# against the deployed ELF a1a0fdc8.../506,164 B and ImageID 7b4040d3....
#
# Cause: the guest ELF embeds each dependency's *materialization path* in its panic-location
# strings. The deployed build carries /root/.cargo/registry/src/index.crates.io-.../<crate-ver>/...
# and /root/.cargo/git/checkouts/<repo>-<hash>/<rev>/... ; `cargo vendor` moves those same files to
# /src/vendor/<crate>/..., and the shorter strings are what the 1,376-byte ELF size delta and the
# 1,388-byte .rodata delta are made of (28 vs 3 "cargo" path strings, 0 vs 21 "registry"). So the
# absolute cargo home is part of the build input, not incidental environment. Keep this mode only
# as the experiment that proves it; the reproduction path is the pinned seeded builder image plus
# the two cargo caches (scripts/build_guest.sh, SEEDED=1 or REGCACHE=).
if [ -f /src/.cargo/config.toml ] && [ -d /src/vendor ]; then
    export CARGO_NET_OFFLINE=true
    du -sh /src/vendor | sed 's/^/VENDORED /'
    du -sh /root/.cargo/git /root/.cargo/registry 2>/dev/null | sed 's/^/CACHES-STILL-CONSUMED-BY /'
    echo "==> VENDORED BUILD: sources from /src/vendor, cargo network disabled"
fi

target_triple="riscv32im-risc0-zkvm-elf"
unit_separator="$(printf '\037')"
guest_rustflags="-C${unit_separator}passes=lower-atomic${unit_separator}-C${unit_separator}link-arg=-Ttext=0x00200800${unit_separator}-C${unit_separator}link-arg=--fatal-warnings${unit_separator}-C${unit_separator}panic=abort${unit_separator}--cfg${unit_separator}getrandom_backend=\"custom\""
export CARGO_ENCODED_RUSTFLAGS="${guest_rustflags}"

echo "==> Building provenance guest"
cargo +risc0 build --release --locked --target "${target_triple}" --manifest-path methods/guest/Cargo.toml

mkdir -p /guest-output
cp "${CARGO_TARGET_DIR}/${target_triple}/release/provenance" /guest-output/provenance.elf
sha256sum /guest-output/provenance.elf
ls -la /guest-output
EOF

FROM scratch AS export
COPY --from=build /guest-output /
