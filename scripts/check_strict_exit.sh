#!/usr/bin/env bash
# Read-only: what do `lezbuild verify` and `verify --strict` actually exit on this tree?
set -uo pipefail
cd /home/user/provenance/tools/lezbuild
BIN=target/debug/lezbuild
test -x "$BIN" || { echo "no binary"; exit 9; }

"$BIN" verify > /tmp/plain.txt 2>&1
echo "PLAIN_EXIT=$?"
"$BIN" verify --strict > /tmp/strict.txt 2>&1
echo "STRICT_EXIT=$?"
"$BIN" verify --strict --manifest /home/user/provenance/artifacts/reproducible-build.json > /tmp/strict2.txt 2>&1
echo "STRICT_EXPLICIT_EXIT=$?"
echo "--- strict head ---"
head -4 /tmp/strict.txt
echo "--- diff of plain vs strict (first 12 lines) ---"
diff /tmp/plain.txt /tmp/strict.txt | head -12 || true
echo "--- grep strict flag in source ---"
grep -n 'strict' src/main.rs
