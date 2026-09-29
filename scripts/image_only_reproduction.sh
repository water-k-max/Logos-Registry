#!/usr/bin/env bash
# The stranger's test, image edition: reproduce the deployed ImageID from the repo + ONE builder
# image, with a brand-new (empty) BuildKit cache, no seed trees in the build context, and no
# container network. If this passes, "pinned image digest" carries the whole dependency story.
set -uo pipefail
SRC=/home/user/provenance
BASE_TAG="${BASE_TAG:-r0.1.88.0-provseed-20260928}"
CACHE_ID="prov-imgonly-$(date +%s)"
CTX=/home/user/prov-build-imgonly
OUT="$SRC/artifacts/verify-image-only"

echo "CACHE_ID=$CACHE_ID"
sed -i 's/\r$//' "$SRC/scripts/build_guest.sh" 2>/dev/null || true
# Delete last run's exports first, and abort on a failed build. Both are needed: `docker --output`
# leaves the exported ELF's mtime at the value recorded inside the image (measured: byte-identical
# mtime across runs), so a stale artifact is otherwise indistinguishable from a fresh one, and a
# passing comparison against it would be a false claim about a build that never happened.
mkdir -p "$OUT" && rm -f "$OUT/provenance.elf" "$OUT/provenance.bin"
SEEDED=1 BASE_TAG="$BASE_TAG" CACHE_ID="$CACHE_ID" NETWORK=none \
  bash "$SRC/scripts/build_guest.sh" "$CTX" "$OUT" || { echo "ABORT: build_guest.sh failed"; exit 1; }
test -f "$OUT/provenance.elf" || { echo "ABORT: build reported success but produced no ELF"; exit 1; }

echo "=== context contents (proof: no gitdb/, no regcache/) ==="
ls -1 "$CTX" | head -20
echo "=== build.log seed/offline evidence ==="
grep -nE 'SEEDED|SEALED|OFFLINE|Compiling|Finished|error|warning: unused' "$OUT/build.log" | head -12
echo "Compiling lines: $(grep -c '^.*Compiling' "$OUT/build.log")"
echo "=== artifacts ==="
sha256sum "$OUT/provenance.elf" "$OUT/provenance.bin"
ls -la "$OUT"

# Expectations come from the manifest, never from a literal typed into this file: a hardcoded
# "expected ELF a1a0fdc8…" keeps printing a confident number long after the artifact it described
# moved, and a reader cannot tell the two cases apart.
read -r EXP_ELF EXP_SIZE EXP_IMAGE < <(python3 -c '
import json, sys
m = json.load(open(sys.argv[1]))["expect"]
print(m["elf_sha256"], m["elf_size_bytes"], m["image_id"])' "$SRC/artifacts/reproducible-build.json")
GOT_ELF=$(sha256sum "$OUT/provenance.elf" | cut -d" " -f1)
GOT_SIZE=$(stat -c %s "$OUT/provenance.elf")
GOT_IMAGE=$(grep -oE "IMAGEID [0-9a-f]{64}" "$OUT/build.log" | tail -1 | cut -d" " -f2)
printf 'manifest expects  elf %s (%s B)  image %s\n' "$EXP_ELF" "$EXP_SIZE" "$EXP_IMAGE"
printf 'this build gave   elf %s (%s B)  image %s\n' "$GOT_ELF" "$GOT_SIZE" "${GOT_IMAGE:-<none>}"
if [ "$GOT_ELF" = "$EXP_ELF" ] && [ "$GOT_SIZE" = "$EXP_SIZE" ] && [ "$GOT_IMAGE" = "$EXP_IMAGE" ]; then
    echo "MATCH: the image-only cold build reproduced the pinned program"
else
    echo "MISMATCH: see artifacts/verify-image-only/build.log"; exit 1
fi
echo
echo "For the full claim — this build, the manifest AND the on-chain Tier-3 record all agreeing:"
echo "  BUILD=1 bash scripts/verify_from_the_outside.sh"
