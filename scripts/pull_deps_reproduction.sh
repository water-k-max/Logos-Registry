#!/usr/bin/env bash
# The null hypothesis about the seeds: are the two cargo caches a build INPUT, or just a
# workaround for this machine's flaky GitHub/crates.io reachability? Nothing is staged, nothing is
# forbidden, fresh BuildKit caches, the pinned UPSTREAM image (3e12f71b…) — cargo has to fetch
# both caches itself. If the ImageID still lands on 7b4040d3…, the seeds are an affordance for
# constrained verifiers and the public input set is repo + image digest.
set -uo pipefail
SRC=/home/user/provenance
CTX=/home/user/prov-build-pull
OUT="$SRC/artifacts/verify-pull"
CACHE_ID="prov-pull-$(date +%s)"

echo "CACHE_ID=$CACHE_ID"
sed -i 's/\r$//' "$SRC/scripts/build_guest.sh" 2>/dev/null || true
PULL_DEPS=1 CACHE_ID="$CACHE_ID" bash "$SRC/scripts/build_guest.sh" "$CTX" "$OUT"
RC=$?
echo "build_guest exit=$RC"
[ "$RC" = 0 ] || { tail -30 "$OUT/build.log" 2>/dev/null; exit "$RC"; }

echo "=== context: nothing staged ==="
ls -1 "$CTX" | grep -E '^(gitdb|regcache|vendor)$' || echo "(no gitdb/, no regcache/, no vendor/)"
echo "=== fetch evidence ==="
grep -nE 'NO-GIT-SEED|UPDATING|Downloading|Compiling risc0|Adding|VENDORED|SEALED|Finished|error' "$OUT/build.log" | head -16
echo "compile lines: $(grep -c 'Compiling' "$OUT/build.log")"
echo "=== comparison ==="
echo "deployed ELF     a1a0fdc88a0e06b82d2e36abc0f3b192fdf8be2914823a8784c5b291e1f56089 506164 B"
echo "deployed bin     6bd3e46c59b50f4be28702811cf69ab0dd28db640425efc287ea0ecb8c2b887b"
echo "deployed IMAGEID 7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6"
sha256sum "$OUT/provenance.elf" "$OUT/provenance.bin" 2>&1
stat -c "%s bytes  %n" "$OUT/provenance.elf" 2>&1
