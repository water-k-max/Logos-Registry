#!/usr/bin/env bash
# Exercise the read-path example end to end: it must compile, print the live entry, and
# return the distinct exit codes the doc promises (0 registered / 3 unregistered / 2 usage).
set -uo pipefail
cd /home/user/provenance/sdk

echo "=== cargo test (pub use AccountId/ProgramId must not break the suite) ==="
cargo test --offline 2>&1 | grep -E 'test result:|^error|no method named|aborting' | head -20

echo "=== build example ==="
cargo build --offline --example explorer-resolve 2>&1 | tail -6

EX=./target/debug/examples/explorer-resolve
echo "=== registered: the deployed registry's own ImageID ==="
$EX 7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6
echo "exit=$?"

echo "=== unregistered: an ImageID nobody has claimed ==="
$EX 0000000000000000000000000000000000000000000000000000000000000001
echo "exit=$?"

echo "=== usage: a flag with no value ==="
$EX --rpc
echo "exit=$?"

echo "=== bad hex ==="
$EX 7b40
echo "exit=$?"
