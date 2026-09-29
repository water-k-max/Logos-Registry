#!/usr/bin/env bash
# Completes the cargo-home experiment's size accounting.
#
# Measured upstream (scripts/cargo_home_sections.sh + cargo_home_strings_by_section.sh):
#   .rodata  27,716 / 27,716 / 27,772  -- 25 materialization-path strings in every arm, 3,100 path
#            bytes in the two 12-char homes, 3,150 in the 14-char one (+50 = 25 x 2 chars, +56 after
#            string-merge alignment). Monotone in path length, exactly as expected.
#   .strtab  123,514 / 123,536 / 123,469 -- contains ZERO cargo-home strings in all three arms, yet
#            it grows 22 bytes in the same-length arm and SHRINKS 45 in the longer-path arm.
#   file     506,164 / 506,188 (+24) / 506,176 (+12); each = section-header offset + 11*40.
#
# So the non-monotonic ELF sizes come from a second, independent term. This script tests the two
# claims that explain it:
#   (1) the names that differ between arms differ ONLY in their trailing ".llvm.<digits>" suffix --
#       strip it and the sets must be identical;
#   (2) that suffix is a decimal number whose digit count varies, so the term is not monotone in
#       anything -- which is precisely why one arm grows while the longer-path arm shrinks.
#
# Scratch files go under the artifact dir, NOT /tmp: an idle WSL distro shuts down and its tmpfs /tmp
# is empty on the next command (measured -- the follow-up pass found no files).
set -uo pipefail
ROOT=/home/user/provenance/artifacts/verify-cargohome
cd "$ROOT" || exit 3
SYMS="$ROOT/syms"
mkdir -p "$SYMS"

# readelf -sW line: "Num: Value Size Type Bind Vis Ndx Name". The Name is everything from field 8 on,
# because mangled Rust names contain spaces inside `$u20$`-escaped generics only in demangled output --
# still, taking the tail is safer than assuming one field.
names() {
    readelf -sW "$1/provenance.elf" | awk '$1 ~ /^[0-9]+:/ {
        name = ""
        for (i = 8; i <= NF; i++) name = name (i > 8 ? " " : "") $i
        if (name != "") print name
    }' | sort > "$SYMS/$1.txt"
}

echo "=== symbol-name totals ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    [ -f "$d/provenance.elf" ] || { echo "  $d missing"; continue; }
    names "$d"
    printf '  %-14s names=%-6s total-name-bytes=%-8s llvm-suffixed=%-6s suffix-digit-sum=%s\n' "$d" \
        "$(wc -l < "$SYMS/$d.txt")" \
        "$(awk '{t += length($0)} END{print t}' "$SYMS/$d.txt")" \
        "$(grep -c '\.llvm\.[0-9]*$' "$SYMS/$d.txt")" \
        "$(grep -o '\.llvm\.[0-9]*$' "$SYMS/$d.txt" | sed 's/^\.llvm\.//' | awk '{t += length($0)} END{print t}')"
done

echo "=== (1) do the arms differ only in the .llvm suffix? ==="
for d in rootcargo rootcarg1 optalt-cargo; do
    sed -E 's/\.llvm\.[0-9]+$//' "$SYMS/$d.txt" | sort -u > "$SYMS/base-$d.txt"
    printf '  %-14s stripped-unique=%s\n' "$d" "$(wc -l < "$SYMS/base-$d.txt")"
done
for d in rootcarg1 optalt-cargo; do
    printf '  %-14s vs control: only-in-control=%s only-in-arm=%s\n' "$d" \
        "$(comm -23 "$SYMS/base-rootcargo.txt" "$SYMS/base-$d.txt" | wc -l)" \
        "$(comm -13 "$SYMS/base-rootcargo.txt" "$SYMS/base-$d.txt" | wc -l)"
done

echo "=== (2) pair each changed name with its control counterpart and count the suffix digits ==="
# base<TAB>suffix, sorted by base, then joined. Pairing the k-th unique name of each set (the first
# attempt) is wrong: both lists are sorted by the FULL name, so the suffix moved the same symbol into
# different positions and the pairs would not be the same symbol.
for d in rootcargo rootcarg1 optalt-cargo; do
    awk '{ if (match($0, /\.llvm\.[0-9]+$/)) { base = substr($0, 1, RSTART-1); sfx = substr($0, RSTART+6) }
           else { base = $0; sfx = "" }
           print base "\t" sfx }' "$SYMS/$d.txt" | sort -t$'\t' -k1,1 > "$SYMS/pairs-$d.txt"
done
for d in rootcarg1 optalt-cargo; do
    join -t$'\t' -j1 "$SYMS/pairs-rootcargo.txt" "$SYMS/pairs-$d.txt" \
    | awk -F'\t' -v arm="$d" '{
          if ($2 != $3) { changed++; digits += length($3) - length($2)
                         if (length($3) > length($2)) longer++; else if (length($3) < length($2)) shorter++
                         if (changed <= 3)
                           printf "      %s\n         ctrl .llvm.%s (%d)  arm .llvm.%s (%d)  delta %+d\n",
                                  substr($1,1,90), $2, length($2), $3, length($3), length($3)-length($2)
          }
      } END {
          printf "  %s: changed=%d  longer=%d  shorter=%d  sum-of-digit-deltas=%+d\n",
                 arm, changed, longer, shorter, digits
      }'
done
echo "  The sum-of-digit-deltas must equal the total-name-bytes delta in the first table, and the"
echo "  name counts stay 5,161 in every arm -- i.e. the sign of that delta is a property of which"
echo "  decimal digit counts the hash happened to land on, not of the path length."
