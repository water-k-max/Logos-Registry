#!/usr/bin/env bash
# Two closing numbers for the cargo-home size accounting:
#   (a) file size == section-header offset + nsections*40 for every arm? then the +24/+12 growth is
#       fully attributed to .rodata + .strtab + the alignment padding between the last section and the
#       section headers, and nothing is left unexplained;
#   (b) how many DISTINCT .llvm.<decimal> values each arm has -- the suffix is per module, so one
#       changed path re-rolls a number that many symbol names then share.
set -uo pipefail
ROOT=/home/user/provenance/artifacts/verify-cargohome
cd "$ROOT" || exit 3
SYMS="$ROOT/syms"

echo "=== (a) file = section-header offset + nsections*40, plus tail padding ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    [ -f "$d/provenance.elf" ] || { echo "  $d missing"; continue; }
    # readelf -h prints this one in DECIMAL followed by "(bytes into file)", so take the first token
    # and do not prefix it with 16#.
    hdr=$(readelf -h -W "$d/provenance.elf" | awk -F': *' '/Start of section headers/{print $2+0; exit}')
    n=$(readelf -h -W "$d/provenance.elf" | awk -F': *' '/Number of section headers/{print $2+0; exit}')
    read -r soff ssize < <(readelf -S -W "$d/provenance.elf" | sed -E 's/\[ *([0-9]+)\]/[\1]/' \
        | awk '$2==".strtab"{print $5, $6}')
    strtab_end=$((16#$soff + 16#$ssize))
    printf '  %-14s file=%-8s hdr_off=%-8s n=%-3s hdr+40n=%-8s strtab_end=%-8s padding=%s\n' \
        "$d" "$(stat -c %s "$d/provenance.elf")" "$hdr" "$n" "$((hdr + n*40))" "$strtab_end" \
        "$((hdr - strtab_end))"
done

echo "=== (b) distinct .llvm suffix values per arm ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    [ -f "$SYMS/$d.txt" ] || { echo "  $d: no name file (run scripts/cargo_home_symboltab.sh)"; continue; }
    printf '  %-14s suffixed-names=%-6s distinct-suffixes=%-6s distinct-digits-sum=%s\n' "$d" \
        "$(grep -c '\.llvm\.[0-9]*$' "$SYMS/$d.txt")" \
        "$(grep -o '\.llvm\.[0-9]*$' "$SYMS/$d.txt" | sort -u | wc -l)" \
        "$(grep -o '\.llvm\.[0-9]*$' "$SYMS/$d.txt" | sed 's/^\.llvm\.//' | sort -u | awk '{t += length($0)} END{print t}')"
done
