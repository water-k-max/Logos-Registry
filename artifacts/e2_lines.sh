#!/usr/bin/env bash
set -uo pipefail
M="$HOME/.cargo/git/checkouts/spel-411f019390120158/1ef0500/spel-framework-macros/src/lib.rs"
grep -n 'fn generate_validation' "$M"
grep -n 'Owner checks first' "$M"
grep -n 'AccountAlreadyInitialized' "$M"
grep -n 'account != nssa_core::account::Account::default()' "$M"
grep -n '#(#signer_checks)\*\|#(#init_checks)\*\|#(#pda_checks)\*\|#(#owner_checks)\*' "$M"
echo "=== is the pinned rev really 1ef0500 in our build?"
grep -n 'spel' /home/user/provenance/Cargo.toml | head
grep -rn 'name = "spel-framework-macros"' -A 3 /home/user/provenance/Cargo.lock | head -8
