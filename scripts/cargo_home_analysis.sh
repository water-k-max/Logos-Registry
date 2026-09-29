#!/usr/bin/env bash
# Read-only analysis of the three cargo-home arms: full hashes, file sizes, and ELF section sizes.
# Section Size is field 8 of `readelf -S -W` ("[ 13] .rodata PROGBITS addr off size ..."), which is
# why an earlier inline awk printed the address instead.
set -uo pipefail
ROOT=/home/user/provenance/artifacts/verify-cargohome
# The control arm IS the deployed program (a1a0fdc8.../7b4040d3..., measured above), so it doubles as
# the reference. artifacts/provenance.elf is not tracked in git (*.bin and experiment scratch are
# ignored), so a reviewer cloning the repo has only what the recipe reproduces -- which is the point
# of shipping the recipe rather than the bytes.
DEPLOYED=$ROOT/rootcargo/provenance.elf

sections() {
    readelf -S -W "$1" | awk '$4==".text"{print "  .text size " $8} $4==".rodata"{print "  .rodata size " $8} $4==".data"{print "  .data size " $8} $4==".bss"{print "  .bss size " $8}'
}
strings_count() {
    tr -c '[:print:]' '\n' < "$1" | grep -c "$2"
}

echo "=== reference = the control arm, which reproduced the deployed hashes ==="
sha256sum "$DEPLOYED" | cut -c1-64
stat -c '  bytes %s' "$DEPLOYED"
sections "$DEPLOYED"

for d in rootcargo rootcarg1 optalt-cargo; do
    [ -f "$ROOT/$d/provenance.elf" ] || { echo "=== $d MISSING ==="; continue; }
    echo "=== arm $d ==="
    sha256sum "$ROOT/$d/provenance.elf" "$ROOT/$d/provenance.bin" | sed 's/^/  /'
    stat -c '  elf bytes %s' "$ROOT/$d/provenance.elf"
    sections "$ROOT/$d/provenance.elf"
    echo "  .bin $ROOT/$d/provenance.bin"
done

echo "=== embedded path strings, per arm (the file!() materialization paths) ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    e="$ROOT/$d/provenance.elf"
    [ -f "$e" ] || continue
    printf '  %-14s registry/src=%s git/checkouts=%s any-/root=%s any-/opt=%s\n' "$d" \
        "$(strings_count "$e" '/registry/src/')" \
        "$(strings_count "$e" '/git/checkouts/')" \
        "$(strings_count "$e" '/root/')" \
        "$(strings_count "$e" '/opt/')"
done

echo "=== unique dependency source paths recorded in each arm (first 3 of each) ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    e="$ROOT/$d/provenance.elf"
    [ -f "$e" ] || continue
    echo "  $d:"
    tr -c '[:print:]' '\n' < "$e" | grep '/registry/src/' | sort -u | head -3 | sed 's/^/    /'
done
