#!/usr/bin/env bash
set -uo pipefail
S="$HOME/.cargo/git/checkouts/spel-411f019390120158/1ef0500/spel-framework-core"
echo "=== definition sites"
grep -rn --include=*.rs 'AccountAlreadyInitialized' "$S" | head -20
echo "=== the check itself"
grep -rn -B 8 -A 12 'AccountAlreadyInitialized' "$S/src" | head -60
echo "=== upstream test"
sed -n '90,125p' "$S/tests/signer_validation.rs"
