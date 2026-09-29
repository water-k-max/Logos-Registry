#!/usr/bin/env bash
# Does the VENDORED input set build the SAME program?
#
# Inputs: repo + `cargo vendor` output + the PINNED UPSTREAM image (r0.1.88.0, digest 3e12f71b…),
# a brand-new BuildKit cache id, and --network none. Neither cargo cache is staged, so if this
# reproduces 7b4040d3… then the dependency set is fully declared content and the 2.2 GB seeded
# image is only a build accelerator.
#
# The risk being tested is byte-identity, not buildability: vendored crates sit at /src/vendor/<name>
# instead of /root/.cargo/registry/src/<hash>/<name>-<version>, so any embedded source path
# (`file!()`, build-script output, `module_path!()` in a panic location) would move the ImageID.
# ANSWER, measured 2026-09-29: NO. The build is clean (166 crates, 1m35s, --network none, FROM the
# pinned upstream digest 3e12f71b… with zero STAGED-FROM-IMAGE lines, so the only variable is where
# the dependency sources sit) and it produces a DIFFERENT program: ImageID
# c9753252bba0ca577e0e580c6f4cdc965e7184e4a4a21600d5e877bc5958a52d, ELF c06db72e…/504,788 B, against
# the deployed 7b4040d3… / a1a0fdc8…/506,164 B. The earlier pre-check that said the deployed ELF
# carries no dependency paths was WRONG — it grepped `^/` and so missed paths that begin mid-string.
# The deployed ELF really does embed /root/.cargo/registry/src/… and /root/.cargo/git/checkouts/…
# (28 and 21 path strings; the vendored build has 25 `vendor` strings instead), and that is the
# whole 1,376-byte delta. The absolute cargo home is a build input. See scripts/vendor_diff_and_seeds.sh.
set -uo pipefail
SRC=/home/user/provenance
VENDOR="${VENDOR:-/home/user/prov-vendor}"
CTX=/home/user/prov-build-vendor
OUT="$SRC/artifacts/verify-vendor"
CACHE_ID="prov-vendor-$(date +%s)"

echo "CACHE_ID=$CACHE_ID"
echo "VENDOR=$VENDOR ($(du -sh "$VENDOR" | cut -f1), $(ls -1 "$VENDOR" | wc -l) package dirs)"
sed -i 's/\r$//' "$SRC/scripts/build_guest.sh" "$SRC/docker/build-guest.Dockerfile" 2>/dev/null || true

VENDOR="$VENDOR" CACHE_ID="$CACHE_ID" NETWORK=none \
  bash "$SRC/scripts/build_guest.sh" "$CTX" "$OUT"
RC=$?
echo "build_guest exit=$RC"
[ "$RC" = 0 ] || exit "$RC"

echo "=== context: proof no cargo cache was staged ==="
ls -1 "$CTX" | grep -E '^(gitdb|regcache|vendor|.cargo)$' || echo "(none of gitdb/regcache present; vendor+.cargo expected)"
echo "=== build.log mode evidence ==="
grep -nE 'NO-GIT-SEED|VENDORED |VENDORED BUILD|SEALED BUILD|SEEDED |CACHES-STILL|Compiling provenance|Finished|error' "$OUT/build.log" | head -14
echo "compile lines: $(grep -c 'Compiling' "$OUT/build.log")"
echo "=== comparison against the deployed artifact ==="
echo "deployed ELF  a1a0fdc88a0e06b82d2e36abc0f3b192fdf8be2914823a8784c5b291e1f56089 506164 B"
echo "deployed bin  6bd3e46c59b50f4be28702811cf69ab0dd28db640425efc287ea0ecb8c2b887b"
echo "deployed IMAGEID 7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6"
sha256sum "$OUT/provenance.elf" "$OUT/provenance.bin"
stat -c "%s bytes  %n" "$OUT/provenance.elf"
if [ -f "$OUT/provenance.bin" ]; then
  /home/user/imgid/target/release/imgid "$OUT/provenance.bin"
fi
