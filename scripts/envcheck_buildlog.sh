#!/usr/bin/env bash
# Confirm the envcheck build was really the cold, sealed path (not a warm-cache no-op).
# `grep -c '^ *Compiling'` returned 0 in the previous pass because BuildKit prefixes every line of a
# RUN step with "<step> <elapsed> ", so the pattern must not anchor at start-of-line.
set -uo pipefail
L=/home/user/provenance/artifacts/verify-outside/build.log
echo "=== file facts ==="
stat -c '  mtime %y  bytes %s' "$L"
echo "=== cold/sealed markers ==="
for pat in 'STAGED-FROM-IMAGE' 'SEEDED' 'SEALED BUILD' 'CARGO_HOME' 'Network' 'FINISHED'; do
    printf '  %-18s %s\n' "$pat" "$(grep -c -- "$pat" "$L")"
done
echo "=== compile evidence (unanchored) ==="
printf '  Compiling lines: %s\n' "$(grep -c 'Compiling' "$L")"
grep -m3 'Compiling' "$L" | sed 's/^/    /'
tail -6 "$L" | sed 's/^/  /'
