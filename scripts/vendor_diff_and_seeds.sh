#!/usr/bin/env bash
# Two measurements, both read/write only under /home/user:
#  (1) WHY the vendored build is a different program — compare embedded source-path strings
#      between the deployed ELF and the vendored one.
#  (2) the deterministic seed bundle: the two cargo caches as one content-addressed file, so the
#      seeded-builder-image channel has an input a stranger can actually be handed.
set -uo pipefail
DEPLOYED=/home/user/provenance/artifacts/verify-image-only/provenance.elf
VENDORED=/home/user/provenance/artifacts/verify-vendor/provenance.elf

echo "=== (1) embedded path strings ==="
for f in "$DEPLOYED" "$VENDORED"; do
  echo "--- $(basename $(dirname $f)) : $(stat -c %s $f) bytes"
  echo "    .rs strings:        $(strings -n 8 "$f" | grep -c '\.rs')"
  echo "    'vendor' strings:   $(strings -n 8 "$f" | grep -c vendor)"
  echo "    '/src/' strings:    $(strings -n 8 "$f" | grep -c '/src/')"
  echo "    'registry' strings: $(strings -n 8 "$f" | grep -c registry)"
  echo "    cargo-path strings: $(strings -n 8 "$f" | grep -c '\.cargo')"
done
echo "--- examples only in the vendored build:"
comm -13 <(strings -n 8 "$DEPLOYED" | sort -u) <(strings -n 8 "$VENDORED" | sort -u) | grep -E '\.rs|vendor|src/' | head -12
echo "--- examples only in the deployed build:"
comm -23 <(strings -n 8 "$DEPLOYED" | sort -u) <(strings -n 8 "$VENDORED" | sort -u) | grep -E '\.rs|registry|src/' | head -12
echo "--- section sizes:"
readelf -S "$DEPLOYED" 2>/dev/null | grep -E '\.rodata|\.text' | head -6
readelf -S "$VENDORED" 2>/dev/null | grep -E '\.rodata|\.text' | head -6

echo
echo "=== (2) deterministic seed bundle ==="
GITDB=/home/user/lez-programs/gitdb
REGCACHE=/home/user/regcache
OUT=/home/user/prov-seeds
mkdir -p "$OUT"
TAR="$OUT/prov-seeds.tar"
rm -f "$TAR" "$TAR.gz" "$TAR.zst"
tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner \
    -C "$(dirname "$GITDB")" -h "$(basename "$GITDB")" \
    -C "$(dirname "$REGCACHE")" -h "$(basename "$REGCACHE")" \
    -cf "$TAR"
echo "tar     $(stat -c %s "$TAR") bytes  sha256 $(sha256sum "$TAR" | cut -d' ' -f1)"
if command -v zstd >/dev/null 2>&1; then
  zstd -q -T0 -19 -o "$TAR.zst" "$TAR" && rm -f "$TAR"
  echo "zstd19  $(stat -c %s "$TAR.zst") bytes  sha256 $(sha256sum "$TAR.zst" | cut -d' ' -f1)"
else
  gzip -n -6 -c "$TAR" > "$TAR.gz" && rm -f "$TAR"
  echo "gzip-6  $(stat -c %s "$TAR.gz") bytes  sha256 $(sha256sum "$TAR.gz" | cut -d' ' -f1)"
fi
