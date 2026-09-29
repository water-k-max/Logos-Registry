#!/usr/bin/env bash
# Full section table for the three cargo-home arms + a size diff against the control.
#
# Why "full": the previous pass grepped only .text/.rodata/.data/.bss/.comment and showed that the
# same-length arm (/root/.carg1) has byte-identical sizes AND offsets for all of those, yet its ELF is
# 24 bytes larger. So the growth must be somewhere that grep filtered out (.symtab/.strtab/.shstrtab,
# the program headers, or padding). This script measures which, instead of asserting it.
#
# Field indices come from the numbered dump: [3]name [5]addr [6]off [7]size.
# Hex arithmetic uses bash's own $((16#..)); Ubuntu's default awk (mawk) has no strtonum().
set -uo pipefail
ROOT=/home/user/provenance/artifacts/verify-cargohome
cd "$ROOT" || exit 3

for d in rootcargo rootcarg1 optalt-cargo; do
    [ -f "$d/provenance.elf" ] || { echo "--- $d: missing"; continue; }
    echo "=== $d  file=$(stat -c %s "$d/provenance.elf")"
    readelf -S -W "$d/provenance.elf" | sed 's/^/    /'
    echo
done

echo "=== section sizes vs control ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    readelf -S -W "$d/provenance.elf" | awk '/^[[:space:]]*\[/{print $3" "$6" "$7}' > "/tmp/chs-$d.txt"
done

# One table per arm, indexed by section number, rather than `paste` + `set --`: inside
# `IFS=$'\t' read` the loop body keeps tab as the only splitter, so words of a "name off size" line
# do not separate and every comparison silently used the whole line. Measured that way once already.
mapfile -t C < /tmp/chs-rootcargo.txt
mapfile -t A < /tmp/chs-rootcarg1.txt
mapfile -t B < /tmp/chs-optalt-cargo.txt
for i in "${!C[@]}"; do
    set -- ${C[$i]}; name="$1"; cs="$3"
    set -- ${A[$i]}; a="$3"
    set -- ${B[$i]}; b="$3"
    if [ "$cs" = "$a" ] && [ "$cs" = "$b" ]; then
        printf '  %-14s size=0x%-8s identical in all three arms\n' "$name" "$cs"
    else
        printf '  %-14s size=0x%-8s .carg1=%+d  /opt=%+d\n' "$name" "$cs" \
            "$((16#$a - 16#$cs))" "$((16#$b - 16#$cs))"
    fi
done

echo "=== offsets of the tail sections (did anything after .comment move?) ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    printf '  %-14s ' "$d"
    awk '{printf "%s@0x%s+0x%s  ", $1, $2, $3}' "/tmp/chs-$d.txt"
    echo
done
