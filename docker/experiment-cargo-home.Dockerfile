# EXPERIMENT, not the recipe. The pinned recipe is docker/build-guest.Dockerfile and this file does
# not touch it.
#
# Purpose: isolate ONE variable. scripts/vendor_reproduction.sh observed that a vendored build of
# identical source produces a different program (ImageID c9753252... vs the deployed 7b4040d3...),
# and the inferred cause was the dependency materialization path that `file!()` embeds in panic
# locations. Observation is not causation: this file rebuilds the same guest N times with identical
# source, lock, base image, rustflags and CARGO_TARGET_DIR, varying only CARGO_HOME. Two of the arms
# use a path of the SAME LENGTH as /root/.cargo so any change cannot be attributed to string length.
#
# No BuildKit cache mounts on purpose: the real recipe's mounts shadow /root/.cargo/{git,registry},
# which is exactly the behaviour that made the seeded-image design necessary (see
# build-guest.Dockerfile lines 26-33). Here every arm starts from an empty cargo home it fills itself.
ARG RISC0_DOCKER_CONTAINER_TAG=r0.1.88.0-provseed-bundle1
FROM risczero/risc0-guest-builder:${RISC0_DOCKER_CONTAINER_TAG} AS build

# ARG -> ENV so the heredoc's shell can actually read it: a bare ARG is not exported into the RUN
# environment, and a silent empty CARGO_HOME would relocate nothing while still printing a result.
ARG ALT_CARGO_HOME=/root/.cargo
ENV ALT_CARGO_HOME=${ALT_CARGO_HOME}

WORKDIR /src
COPY . .

# Byte-identical to the pinned recipe's environment block.
ENV CARGO_TARGET_DIR=/src/target/prov-guest
ENV RISC0_FEATURE_bigint2=""
ENV CC_riscv32im_risc0_zkvm_elf=/root/.risc0/cpp/bin/riscv32-unknown-elf-gcc
ENV CFLAGS_riscv32im_risc0_zkvm_elf="-march=rv32im -nostdlib"

RUN <<'EOF'
set -eu

export CARGO_HOME="${ALT_CARGO_HOME:?ALT_CARGO_HOME is empty - the arm would not be relocated}"
echo "ARM-CARGO-HOME $CARGO_HOME"

mkdir -p "$CARGO_HOME/git/db" "$CARGO_HOME/registry"
cp -a /opt/prov-seed/gitdb/. "$CARGO_HOME/git/db/"
cp -a /opt/prov-seed/regcache/. "$CARGO_HOME/registry/"
du -sh "$CARGO_HOME/git/db" "$CARGO_HOME/registry" | sed 's/^/ARM-SEED /'

export CARGO_NET_OFFLINE=true

target_triple="riscv32im-risc0-zkvm-elf"
unit_separator="$(printf '\037')"
guest_rustflags="-C${unit_separator}passes=lower-atomic${unit_separator}-C${unit_separator}link-arg=-Ttext=0x00200800${unit_separator}-C${unit_separator}link-arg=--fatal-warnings${unit_separator}-C${unit_separator}panic=abort${unit_separator}--cfg${unit_separator}getrandom_backend=\"custom\""
export CARGO_ENCODED_RUSTFLAGS="${guest_rustflags}"

cargo +risc0 build --release --locked --target "${target_triple}" --manifest-path methods/guest/Cargo.toml

mkdir -p /guest-output
cp "${CARGO_TARGET_DIR}/${target_triple}/release/provenance" /guest-output/provenance.elf
sha256sum /guest-output/provenance.elf
# How many embedded dependency paths moved with the cargo home, and where they point.
echo "ARM-PATHSTRINGS $(tr -c '[:print:]' '\n' < /guest-output/provenance.elf | grep -c "$CARGO_HOME/registry/src" || true)"
EOF

FROM scratch AS export
COPY --from=build /guest-output /
