#!/usr/bin/env bash
# Sharpens the same-length-arm claim from "identical .rodata size" to a byte-level statement.
# If /root/.carg1 (12 chars, like /root/.cargo) really only renames one character per embedded path,
# then its .rodata must be the same length with a small, localised set of differing bytes -- and the
# ImageID difference therefore cannot be blamed on size or layout at all, only on string CONTENT.
set -uo pipefail
ROOT=/home/user/provenance/artifacts/verify-cargohome
cd "$ROOT" || exit 3

rodata() {
    read -r off size < <(readelf -S -W "$1/provenance.elf" | sed -E 's/\[ *([0-9]+)\]/[\1]/' \
        | awk '$2==".rodata"{print $5, $6}')
    dd if="$1/provenance.elf" of="$2" bs=1 skip=$((16#$off)) count=$((16#$size)) status=none
}
text() {
    read -r off size < <(readelf -S -W "$1/provenance.elf" | sed -E 's/\[ *([0-9]+)\]/[\1]/' \
        | awk '$2==".text"{print $5, $6}')
    dd if="$1/provenance.elf" of="$2" bs=1 skip=$((16#$off)) count=$((16#$size)) status=none
}

for d in rootcargo rootcarg1 optalt-cargo; do
    rodata "$d" "/tmp/rd-$d.bin"
    text "$d" "/tmp/tx-$d.bin"
done

echo "=== .rodata vs control ==="
for d in rootcarg1 optalt-cargo; do
    if cmp -s /tmp/rd-rootcargo.bin "/tmp/rd-$d.bin"; then
        echo "  $d: byte-identical"
    else
        printf '  %-14s sizes=%s/%s  differing_bytes=%s  first_diff_offset=%s\n' "$d" \
            "$(stat -c %s /tmp/rd-rootcargo.bin)" "$(stat -c %s /tmp/rd-$d.bin)" \
            "$(cmp -l /tmp/rd-rootcargo.bin "/tmp/rd-$d.bin" 2>/dev/null | wc -l)" \
            "$(cmp /tmp/rd-rootcargo.bin "/tmp/rd-$d.bin" 2>/dev/null | awk '{print $4}' | head -1)"
    fi
done

echo "=== .text vs control (does the instruction stream move at all?) ==="
for d in rootcarg1 optalt-cargo; do
    if cmp -s /tmp/tx-rootcargo.bin "/tmp/tx-$d.bin"; then
        echo "  $d: .text byte-identical ($(stat -c %s /tmp/tx-$d.bin) bytes)"
    else
        printf '  %-14s .text differs in %s bytes\n' "$d" \
            "$(cmp -l /tmp/tx-rootcargo.bin "/tmp/tx-$d.bin" 2>/dev/null | wc -l)"
    fi
done
