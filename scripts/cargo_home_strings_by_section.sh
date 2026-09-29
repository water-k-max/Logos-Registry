#!/usr/bin/env bash
# Where exactly do the cargo-home path strings live, and how many bytes do they account for?
#
# Follow-up to scripts/cargo_home_sections.sh, which measured:
#   .rodata   control 0x006c44 / .carg1 0x006c44 (+0)  /opt 0x006c7c (+56)
#   .strtab   control 0x1e27b / .carg1 0x1e290 (+21)  /opt 0x1e24d (-46)
#   file      control 506164 / .carg1 +24 / /opt +12, and each file size = section-header offset
#             + 11*40, with 2-3 bytes of alignment padding at the end. So the totals are explained,
#             but not the *cause*: one section grew with path length while the other shrank.
#
# This script extracts the two sections by offset+size and counts, inside each, the strings that
# contain the arm's own cargo home. No guessing about which section carries what.
#
# readelf field indices are normalised first: "[ 8]" splits into two awk fields but "[10]" into one,
# which is what made the earlier inline attempts read the address column as a size.
set -uo pipefail
ROOT=/home/user/provenance/artifacts/verify-cargohome
cd "$ROOT" || exit 3

# The bracket-normalising sed uses -E: the same pattern in BRE failed on this run as
# "Unmatched ( or \(". It rewrites "[ 8]" to "[8]" so one-digit and two-digit section numbers split
# into the same awk fields ([1]=Nr [2]=name [3]=type [4]=addr [5]=off [6]=size).
table() { readelf -S -W "$1" | sed -E 's/\[ *([0-9]+)\]/[\1]/' | awk '/^[[:space:]]*\[[0-9]+\]/{print $2" "$5" "$6}'; }
# $1 = elf, $2 = section name -> prints "offset size"
field() { table "$1" | awk -v n="$2" '$1==n{print $2, $3}'; }

extract() { # elf section outfile
    local off size
    read -r off size < <(field "$1" "$2")
    [ -n "${off:-}" ] || { echo "no section $2 in $1" >&2; return 1; }
    dd if="$1" of="$3" bs=1 skip=$((16#$off)) count=$((16#$size)) status=none
    printf '%s' "$((16#$size))"
}

for arm in rootcargo rootcarg1 optalt-cargo; do
    e="$arm/provenance.elf"
    case $arm in
        rootcargo)    home='/root/.cargo'    ;;
        rootcarg1)    home='/root/.carg1'    ;;
        optalt-cargo) home='/opt/alt-cargo'  ;;
    esac
    echo "=== $arm  (cargo home $home, ${#home} chars)"
    printf '    file bytes      %s\n' "$(stat -c %s "$e")"
    for sec in .rodata .strtab .text; do
        out="/tmp/sec-$arm-$sec.bin"
        sz=$(extract "$e" "$sec" "$out") || continue
        # Byte total of the section, count of strings containing the home path, and the sum of their
        # lengths: that is the section's *path content footprint*. Plain substitution, no eval — the
        # script body never passes through the Windows shell layer, so `$home` is safe here.
        n=$(tr -c '[:print:]' '\n' < "$out" | grep -c -- "$home")
        b=$(tr -c '[:print:]' '\n' < "$out" | grep -- "$home" | awk '{t += length($0)} END{print t+0}')
        printf '    %-9s size=%-8s path-strings=%-4s path-bytes=%s\n' "$sec" "$sz" "$n" "$b"
    done
    printf '    whole-file path-strings=%s\n' "$(tr -c '[:print:]' '\n' < "$e" | grep -c "$home")"
done
