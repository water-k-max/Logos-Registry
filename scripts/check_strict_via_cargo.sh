#!/usr/bin/env bash
# Read-only: does `cargo run -- verify --strict` behave like the built binary?
# The exit code is the only observable difference, so measure it both ways.
set -uo pipefail
cd /home/user/provenance/tools/lezbuild

cargo build --offline -q
./target/debug/lezbuild verify --strict > /tmp/bin_strict.txt 2>&1
echo "BIN_STRICT_EXIT=$?"

cargo run --offline -q -- verify --strict > /tmp/cargo_strict.txt 2>&1
echo "CARGO_STRICT_EXIT=$?"

cargo run --offline -q -- verify "--strict" > /tmp/cargo_strict2.txt 2>&1
echo "CARGO_STRICT2_EXIT=$?"

echo "--- args seen by the cargo-run invocation (via a print) ---"
grep -c 'informational' /tmp/cargo_strict.txt /tmp/bin_strict.txt
