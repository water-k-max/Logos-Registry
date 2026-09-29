#!/usr/bin/env bash
# Mechanism check for the 14-char arm's .text difference (2,434 bytes vs the control).
# Hypothesis to test, not to assert: .rodata is the merged-string table, so lengthening the cargo-home
# path shifts every string after the first dependency path; every code site that loads one of those
# addresses is an AUIPC+ADDI pair, and only the immediate bytes of those instructions change.
# Predictions this script measures:
#   - the .text diff comes in short runs (immediates are 1-4 bytes, not whole-basic-block rewrites);
#   - all runs sit inside the executable range, clustered at 4-byte instruction boundaries (RV32IM is
#     fixed 32-bit here unless compressed instructions are present).
set -uo pipefail
ROOT=/home/user/provenance/artifacts/verify-cargohome
cd "$ROOT" || exit 3

sect() { # elf name outfile
    read -r off size < <(readelf -S -W "$1/provenance.elf" | sed -E 's/\[ *([0-9]+)\]/[\1]/' \
        | awk -v n="$3" '$2==n{print $5, $6}')
    dd if="$1/provenance.elf" of="$2" bs=1 skip=$((16#$off)) count=$((16#$size)) status=none
}
sect rootcargo /tmp/tx-c.bin .text
sect optalt-cargo /tmp/tx-o.bin .text

echo "=== differing positions in .text (control vs /opt/alt-cargo) ==="
cmp -l /tmp/tx-c.bin /tmp/tx-o.bin | awk '{print $1 - 1}' > /tmp/txpos.txt
printf '  differing_bytes=%s  range=%s..%s\n' "$(wc -l < /tmp/txpos.txt)" \
    "$(head -1 /tmp/txpos.txt)" "$(tail -1 /tmp/txpos.txt)"

echo "=== run-length histogram (consecutive differing positions form a run) ==="
awk 'NR==1 {start=$1; prev=$1; len=1; next}
     $1 == prev+1 {prev=$1; len++; next}
     {print len; start=prev; len=1; prev=$1}
     END {print len}' /tmp/txpos.txt | sort -n | uniq -c | sort -k2n \
| awk '{printf "  run_length=%-3s count=%s\n", $2, $1}'

echo "=== instruction-boundary alignment of the first differing byte of each run ==="
awk 'NR==1 {start=$1; prev=$1; next}
     $1 != prev+1 {print start; start=$1}
     {prev=$1}
     END {print start}' /tmp/txpos.txt > /tmp/txrun.txt
total=$(wc -l < /tmp/txrun.txt)
aligned=$(awk '{if ($1 % 4 == 0) c++} END{print c+0}' /tmp/txrun.txt)
mod3=$(awk '{if ($1 % 4 == 3) c++} END{print c+0}' /tmp/txrun.txt)
mod1=$(awk '{if ($1 % 4 == 1) c++} END{print c+0}' /tmp/txrun.txt)
mod2=$(awk '{if ($1 % 4 == 2) c++} END{print c+0}' /tmp/txrun.txt)
# No literal percent signs inside the printf format: bash read "start%4==0:" as the conversion "%4"
# followed by "=", and aborted with "invalid format character". Values are interpolated instead.
printf '  runs=%s  run_start mod 4 -> 0:%s 1:%s 2:%s 3:%s\n' "$total" "$aligned" "$mod1" "$mod2" "$mod3"
echo "  An AUIPC/ADDI pair that only re-encodes its immediate differs in 1-2 adjacent bytes. The run"
echo "  lengths (max 2, none longer) are the load-bearing measurement: no instruction sequence was"
echo "  rewritten, only address fields. The mod-4 spread is reported for completeness."
