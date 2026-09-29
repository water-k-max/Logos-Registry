#!/usr/bin/env bash
set -uo pipefail
S="$HOME/.cargo/git/checkouts/spel-411f019390120158/1ef0500"
echo "=== head of the test file (is __validate_create_state hand-written or generated?)"
sed -n '1,60p' "$S/spel-framework-core/tests/signer_validation.rs"
echo "=== does the framework SRC implement the init check anywhere?"
grep -rn --include=*.rs 'AccountAlreadyInitialized' "$S" | grep -v '/tests/' | head -20
echo "=== search all spel revs for the init validator (already_initialized / is_initialized / has_data)"
grep -rn --include=*.rs -iE 'already_initialized|is_initialized|account_data.is_empty|len\(\) == 0' "$S" | grep -v '/tests/' | head -25
